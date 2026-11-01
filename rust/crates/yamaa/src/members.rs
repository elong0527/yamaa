//! The members of a project run built at once (#4193). Members whose inputs
//! read no other member's artifact build together, each on a thread of its
//! own, and a member that reads another's artifact starts once that
//! producer has finished. The calling thread publishes every member in run
//! order, so the outcome, its issues, its reports and the progress events
//! are those of a run that builds one member at a time.
//!
//! - **Threads.** The members building at once and their partitions share
//!   the engine's partition threads (the `threads` the hosts and the
//!   command line take): a member starts while a thread is free, and its
//!   partitions take an equal share of the free threads among the members
//!   ready to start, at least one, which return to the budget when it ends.
//!   So the members building at once and their partitions never hold more
//!   than those threads.
//! - **Progress.** A member on a thread reports its events to a reporter of
//!   its own, which keeps them; the calling thread reports them to the
//!   operation's callback as it publishes the member, in run order, so the
//!   callback runs on the calling thread alone. Once the callback has
//!   failed no later member is published, as no later member builds on one
//!   thread, and a plan a member compiled is kept only once it is
//!   published, as on one thread.
//! - **Project functions.** A member whose specification calls a project
//!   function compiles on a thread, which hands its plan back; the calling
//!   thread runs it in its turn, once every member before it is published,
//!   holding one thread of the budget, so its activation and calls run on
//!   the calling thread (FUNC-8).
//! - **Cancellation.** The calling thread alone polls the operation's
//!   token, whose probe a host answers on its own thread, at least every
//!   [`POLL`] while it waits, and every member's thread polls a
//!   [`CancellationToken::follower`] of it at its own checkpoints. A
//!   cancelled run keeps nothing.
//! - **Panics.** A member's thread that panics stops the others, and its
//!   panic is raised again on the calling thread once they have ended.

use std::any::Any;
use std::num::NonZeroUsize;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Arc;
use std::time::Duration;

use yamaa_adapters::file_resources::HeldArtifacts;

use crate::api::{Admission, Node, Plans};
use crate::cancel::{CancellationToken, Cancelled};
use crate::engine::{Build, Engine};
use crate::internal::RunInputs;
use crate::outcome::Member;
use crate::plan::Plan;
use crate::progress::{Measured, ProgressEvent, Reporter};

/// How long the calling thread waits for a member before it polls the
/// cancellation token again.
const POLL: Duration = Duration::from_millis(10);

/// What a member's thread hands back.
enum Sent {
    /// The member's build; the plan it compiled, which the holder keeps once
    /// the member is published; and the progress events it reported, in
    /// order.
    Built {
        build: Box<Build>,
        compiled: Option<Plan>,
        events: Vec<ProgressEvent>,
    },
    /// The plan of a member whose run calls project functions, which the
    /// calling thread runs; whether it was compiled now; what its timings
    /// are read from; and the events its compilation reported.
    Host {
        plan: Plan,
        compiled: bool,
        measured: Measured,
        events: Vec<ProgressEvent>,
    },
    /// The build was cancelled.
    Cancelled,
    /// The thread panicked, with this payload.
    Panicked(Box<dyn Any + Send>),
}

/// One member as the calling thread follows it.
enum Slot {
    /// Waiting for a producer before it to finish.
    Waiting,
    /// It builds once a thread is free.
    Ready,
    /// Building on a thread.
    Running,
    /// Ready to publish: refused, built, or a plan for the calling thread.
    Done(Sent),
    Published,
}

/// How a pass of the calling thread's loop ended.
enum Ended {
    /// Every member to publish is published.
    Published,
    Cancelled,
    Panicked(Box<dyn Any + Send>),
}

/// What the calling thread holds while the members build.
struct Schedule {
    slots: Vec<Slot>,
    /// Whether each member completed, with an artifact; none until known.
    completed: Vec<Option<bool>>,
    /// The artifacts of the members that completed, for their consumers.
    held: HeldArtifacts,
    /// The threads of the budget no member holds.
    free: usize,
    /// The members published so far, in run order.
    members: Vec<Member>,
}

/// Cancels a token once dropped: a scope that ends, or unwinds, stops the
/// members still building.
struct StopOnDrop<'a>(&'a CancellationToken);

impl Drop for StopOnDrop<'_> {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

/// The members of `nodes`, in run order, built at once on up to `threads`
/// threads, as [`Engine::run_graph`] publishes them.
pub(crate) fn build(
    engine: &Engine,
    nodes: &[Node<'_>],
    plans: &Plans,
    reporter: &Reporter,
    token: &CancellationToken,
    threads: NonZeroUsize,
) -> Result<Vec<Member>, Cancelled> {
    let mut schedule = Schedule {
        slots: nodes.iter().map(|_| Slot::Waiting).collect(),
        completed: vec![None; nodes.len()],
        held: HeldArtifacts::new(),
        free: threads.get(),
        members: Vec::with_capacity(nodes.len()),
    };
    let stop = token.follower();
    let (sender, receiver) = mpsc::channel::<(usize, usize, Sent)>();
    let ended = std::thread::scope(|scope| {
        let _stop = StopOnDrop(&stop);
        loop {
            if token.is_cancelled() {
                break Ended::Cancelled;
            }
            schedule.admit(engine, nodes);
            match schedule.publish(engine, nodes, plans, reporter, token) {
                Ok(true) => break Ended::Published,
                Ok(false) => {}
                Err(Cancelled) => break Ended::Cancelled,
            }
            for (position, share) in schedule.dispatch() {
                let (node, sender, stop) = (&nodes[position], sender.clone(), stop.clone());
                let held = Arc::new(schedule.held.clone());
                scope.spawn(move || {
                    let sent = catch_unwind(AssertUnwindSafe(|| {
                        on_thread(engine, node, plans, reporter, held, share, &stop)
                    }))
                    .unwrap_or_else(Sent::Panicked);
                    // The calling thread stops listening only once it ends.
                    let _ = sender.send((position, share.get(), sent));
                });
            }
            match receiver.recv_timeout(POLL) {
                Ok((position, share, sent)) => {
                    if let Some(ended) = schedule.receive(nodes, position, share, sent) {
                        break ended;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => unreachable!("the calling thread sends too"),
            }
        }
    });
    match ended {
        Ended::Published => Ok(schedule.members),
        Ended::Cancelled => Err(Cancelled),
        Ended::Panicked(payload) => resume_unwind(payload),
    }
}

/// Build one member on its thread: compile it, or reuse the plan `plans`
/// keeps, reporting to a reporter of its own; then run it with its share
/// of the threads, unless it calls project functions.
fn on_thread(
    engine: &Engine,
    node: &Node<'_>,
    plans: &Plans,
    reporter: &Reporter,
    held: Arc<HeldArtifacts>,
    threads: NonZeroUsize,
    token: &CancellationToken,
) -> Sent {
    if token.is_cancelled() {
        return Sent::Cancelled;
    }
    let (deferred, heard) = reporter.deferred();
    let observed = deferred.observe(node.observed_name());
    let (plan, compiled) = engine.plan_reusing(plans, &node.request, node.plan.as_ref(), &observed);
    if plan.calls_functions(engine.shared()) {
        return Sent::Host {
            plan,
            compiled,
            measured: observed.measured().clone(),
            events: heard.take(),
        };
    }
    let build = plan.run_through(
        engine.shared(),
        RunInputs {
            held: Some(held),
            frames: node.request.inputs.cloned().unwrap_or_default(),
            threads: Some(threads),
        },
        &observed,
        token,
    );
    match Engine::finish(build, &plan, &node.request, &observed, token) {
        Ok(build) => Sent::Built {
            build: Box::new(build),
            compiled: compiled.then_some(plan),
            events: heard.take(),
        },
        Err(Cancelled) => Sent::Cancelled,
    }
}

impl Schedule {
    /// Decide each waiting member whose producers before it have finished:
    /// it builds, or its findings refuse it, as on one thread.
    fn admit(&mut self, engine: &Engine, nodes: &[Node<'_>]) {
        for (position, node) in nodes.iter().enumerate() {
            if !matches!(self.slots[position], Slot::Waiting) {
                continue;
            }
            match node.admission(position, |producer| self.completed[producer]) {
                Admission::Waiting => {}
                Admission::Builds => self.slots[position] = Slot::Ready,
                Admission::Refused(issues) => {
                    let build = node.refused_build(engine, issues);
                    self.finished(nodes, position, &build);
                    self.slots[position] = Slot::Done(Sent::Built {
                        build: Box::new(build),
                        compiled: None,
                        events: Vec::new(),
                    });
                }
            }
        }
    }

    /// Record that the member at `position` finished as `build`, holding its
    /// artifact for its consumers.
    fn finished(&mut self, nodes: &[Node<'_>], position: usize, build: &Build) {
        let artifact = build.artifact();
        self.completed[position] = Some(artifact.is_some());
        if let (Some(bytes), Some((producer, location))) = (artifact, &nodes[position].artifact) {
            self.held.insert(producer.clone(), location.clone(), bytes);
        }
    }

    /// The members that start now, each with its share of the free
    /// threads: an equal share among the members ready to start, at least
    /// one, in run order while a thread is free.
    fn dispatch(&mut self) -> Vec<(usize, NonZeroUsize)> {
        let mut ready = self
            .slots
            .iter()
            .filter(|slot| matches!(slot, Slot::Ready))
            .count();
        let mut started = Vec::new();
        for (position, slot) in self.slots.iter_mut().enumerate() {
            if self.free == 0 || ready == 0 {
                break;
            }
            if !matches!(slot, Slot::Ready) {
                continue;
            }
            let share = (self.free / ready).max(1);
            self.free -= share;
            ready -= 1;
            *slot = Slot::Running;
            started.push((
                position,
                NonZeroUsize::new(share).expect("a share is one or more"),
            ));
        }
        started
    }

    /// Take back what the member at `position`'s thread sent, returning its
    /// `share` of the threads; a cancelled or panicked thread ends the run.
    fn receive(
        &mut self,
        nodes: &[Node<'_>],
        position: usize,
        share: usize,
        sent: Sent,
    ) -> Option<Ended> {
        self.free += share;
        match sent {
            Sent::Cancelled => return Some(Ended::Cancelled),
            Sent::Panicked(payload) => return Some(Ended::Panicked(payload)),
            Sent::Built { ref build, .. } => self.finished(nodes, position, build),
            Sent::Host { .. } => {}
        }
        self.slots[position] = Slot::Done(sent);
        None
    }

    /// Publish the members that are done, in run order, as the calling
    /// thread of a run on one thread would: true once every member to
    /// publish is published, which is none after the progress callback
    /// failed. A member whose run calls project functions runs here, in its
    /// turn, once a thread of the budget is free.
    fn publish(
        &mut self,
        engine: &Engine,
        nodes: &[Node<'_>],
        plans: &Plans,
        reporter: &Reporter,
        token: &CancellationToken,
    ) -> Result<bool, Cancelled> {
        let mut position = self.members.len();
        while position < nodes.len() {
            let node = &nodes[position];
            // A failed callback publishes no later member, whether or not it
            // is done, so the run stops starting members at once.
            if reporter.failed() {
                return Ok(true);
            }
            let ready = match &self.slots[position] {
                Slot::Done(Sent::Host { .. }) => self.free > 0,
                Slot::Done(_) => true,
                _ => false,
            };
            if !ready {
                return Ok(false);
            }
            let Slot::Done(sent) = std::mem::replace(&mut self.slots[position], Slot::Published)
            else {
                unreachable!("a member to publish is done");
            };
            let build = match sent {
                Sent::Built {
                    build,
                    compiled,
                    events,
                } => {
                    reporter.replay(events);
                    if let Some(plan) = compiled {
                        engine.keep(plans, &node.request, node.plan.clone(), &plan);
                    }
                    *build
                }
                Sent::Host {
                    plan,
                    compiled,
                    measured,
                    events,
                } => {
                    reporter.replay(events);
                    let build = self.on_calling_thread(
                        engine,
                        node,
                        (plans, &plan, compiled),
                        reporter.resume(measured),
                        token,
                    )?;
                    self.finished(nodes, position, &build);
                    build
                }
                Sent::Cancelled | Sent::Panicked(_) => unreachable!("a run that stopped ended"),
            };
            self.members.push(node.member(build));
            position += 1;
        }
        Ok(true)
    }

    /// Run a member whose run calls project functions on the calling thread,
    /// holding one thread of the budget, as a run on one thread builds it.
    fn on_calling_thread(
        &mut self,
        engine: &Engine,
        node: &Node<'_>,
        (plans, plan, compiled): (&Plans, &Plan, bool),
        observed: crate::progress::Observed<'_>,
        token: &CancellationToken,
    ) -> Result<Build, Cancelled> {
        self.free -= 1;
        let build = plan.run_through(
            engine.shared(),
            RunInputs {
                held: Some(Arc::new(self.held.clone())),
                frames: node.request.inputs.cloned().unwrap_or_default(),
                threads: Some(NonZeroUsize::MIN),
            },
            &observed,
            token,
        );
        self.free += 1;
        if compiled {
            engine.keep(plans, &node.request, node.plan.clone(), plan);
        }
        Engine::finish(build, plan, &node.request, &observed, token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The split of the threads the module states: in run order, while a
    /// thread is free, each member ready to start takes the free threads
    /// divided by the members still ready, rounded down, at least one. The
    /// expected shares are worked by hand from that rule.
    #[test]
    fn members_ready_at_once_share_the_free_threads_at_least_one_each() {
        for (free, ready, expected) in [
            (4, 1, vec![4]),
            (4, 3, vec![1, 1, 2]),
            (4, 9, vec![1, 1, 1, 1]),
            (8, 3, vec![2, 3, 3]),
            (1, 2, vec![1]),
            (0, 2, vec![]),
        ] {
            let mut schedule = Schedule {
                slots: (0..ready).map(|_| Slot::Ready).collect(),
                completed: vec![None; ready],
                held: HeldArtifacts::new(),
                free,
                members: Vec::new(),
            };
            let shares = schedule
                .dispatch()
                .into_iter()
                .map(|(_, share)| share.get())
                .collect::<Vec<_>>();
            let label = format!("{free} free threads, {ready} members ready");
            assert_eq!(shares, expected, "{label}");
            assert_eq!(
                schedule.free,
                free - expected.iter().sum::<usize>(),
                "{label}: the threads left free"
            );
        }
    }
}
