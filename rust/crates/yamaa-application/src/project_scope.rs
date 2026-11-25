//! Project operations over the specifications a project file lists: their
//! names, the run scope and producer-first order of a project run, and the
//! project findings. Specifications still check and build through the
//! ordinary entry one at a time; this module reads only the project file and
//! specification metadata.
use crate::{
    file_resources::{same_location, same_spelling, Error as ResourceError, Resources},
    issue_location::Locator,
    public_table::PublicTable,
};
use std::sync::Arc;
use yamaa_core::application_issue::{self as issue, Fault};
use yamaa_core::diagnostic::Diagnostic;
use yamaa_core::json::Value;
use yamaa_core::project_environment::Submission;
use yamaa_core::project_function::Language;
use yamaa_core::project_scope as scope;
use yamaa_core::schema::{Document, DocumentNode as N};

/// A file location together with the path its specification wrote and the
/// layer that wrote it.
#[derive(Clone, Debug)]
struct Located {
    location: String,
    written: String,
    source: String,
}

#[derive(Clone, Debug)]
struct Input {
    dataset: String,
    path: Option<Located>,
    schema: Option<Schema>,
}

/// An input's `schema` link: the identity of the producer it names and of the
/// layer that wrote it.
#[derive(Clone, Debug)]
struct Schema {
    producer: String,
    source: String,
}

/// An in-scope producer link: the consuming input, the producer listing and
/// the layer that wrote the input's `schema`.
#[derive(Clone, Debug)]
struct Link {
    dataset: String,
    producer: usize,
    source: String,
}

/// One specification a section lists, with the metadata project operations
/// read from it. A listing that cannot be read keeps its location and the
/// findings that explain it, and has no name; one whose specification cannot
/// be prepared keeps the `domain` its own file writes as its name.
#[derive(Clone, Debug)]
pub struct Listed {
    section: Submission,
    index: usize,
    /// The path as the project file writes it.
    written: String,
    location: String,
    identity: Option<String>,
    name: Option<String>,
    inputs: Vec<Input>,
    output: Option<Located>,
    report: Option<Located>,
    /// Findings that fail the listing before its specification is read.
    failure: Vec<Diagnostic>,
    /// The origins its section's Define-XML 2.0 document cannot record
    /// (DEFINE-29), which a project check reports (PROJECT-7).
    unrepresentable: Vec<Diagnostic>,
}
impl Listed {
    /// The listing at `section.specs[index]`, which writes `written`, before
    /// its specification is read.
    fn unread(section: Submission, index: usize, written: String) -> Self {
        Self {
            section,
            index,
            location: written.clone(),
            written,
            identity: None,
            name: None,
            inputs: Vec::new(),
            output: None,
            report: None,
            failure: Vec::new(),
            unrepresentable: Vec::new(),
        }
    }
    /// The specification file, as checks and builds receive it.
    pub fn path(&self) -> &str {
        &self.location
    }
    /// The specification file as the project file writes it, such as
    /// `adam/adsl.yaml`.
    pub fn written(&self) -> &str {
        &self.written
    }
    /// The canonical identity of the specification, or its location when the
    /// specification could not be read.
    pub fn identity(&self) -> &str {
        self.identity.as_deref().unwrap_or(&self.location)
    }
    /// The specification's `domain`, its name in the project.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
    pub fn section(&self) -> Submission {
        self.section
    }
    /// The project path of the listing, such as `sdtm.specs[0]`.
    pub fn listing(&self) -> String {
        format!("{}.specs[{}]", scope::key(self.section), self.index)
    }
    /// Whether any input links its producer with `schema`.
    pub fn links_producers(&self) -> bool {
        self.inputs.iter().any(|input| input.schema.is_some())
    }
    /// The location of the primary artifact `output.path` names.
    pub fn artifact_location(&self) -> Option<&str> {
        self.output.as_ref().map(|output| output.location.as_str())
    }
    /// Findings that fail the listing before its specification is read.
    pub fn failure(&self) -> &[Diagnostic] {
        &self.failure
    }
    fn key_matches(&self, key: &str) -> bool {
        same_spelling(self.identity(), key) || same_spelling(&self.location, key)
    }
}

/// One project file with every specification its sections list.
pub struct Project {
    identity: String,
    /// Declared sections in run order, each with whether it names `define`.
    sections: Vec<(Submission, bool)>,
    /// Every listing, in section run order and then listing order.
    listed: Vec<Listed>,
    /// The project file, the files it reads and each listed specification
    /// that was read: what locates the project findings (P-ISSUE-1).
    locator: Arc<Locator>,
}

/// Name `entry` as the listed specification an issue belongs to, and as the
/// document it comes from when the issue names none, as multi-specification
/// checks do.
pub fn identify(issues: &mut [Diagnostic], entry: &str) {
    for issue in issues {
        for key in ["source", "entry"] {
            issue
                .context
                .entry(key.to_owned())
                .or_insert_with(|| Value::String(entry.into()));
        }
    }
}

fn identified(diagnostic: Diagnostic, entry: &str) -> Diagnostic {
    let mut issues = [diagnostic];
    identify(&mut issues, entry);
    let [issue] = issues;
    issue
}

/// Name `entry` as the listed specification a finding belongs to, and
/// `source`, the layer that wrote the field it is about, as the document it
/// comes from, which may be a layer above the entry (P-ISSUE-1).
fn identified_in(mut diagnostic: Diagnostic, entry: &str, source: &str) -> Diagnostic {
    diagnostic
        .context
        .insert("source".into(), Value::String(source.into()));
    identified(diagnostic, entry)
}

fn text(document: &Document, node: usize) -> Option<&str> {
    match &document.nodes()[node] {
        N::Text(text) => Some(text),
        _ => None,
    }
}

/// The refusal of the specification a project lists at `listing` as
/// `written`. A path its written form refuses, or one that reaches no entry,
/// a directory or another entry that is not a regular file, or a link, fails
/// with its file rule's condition at the listing, naming the path as written,
/// as a listed function or codelist file does (FILE-11, #3030 D107); any
/// other failure stays the opaque resource refusal there.
fn listing_failure(listing: &str, written: &str, error: &ResourceError) -> Vec<Diagnostic> {
    use yamaa_core::resource::ResourceFailure as R;
    if let Some(
        failure @ (R::UriScheme
        | R::Backslash
        | R::EmptySegment
        | R::Missing
        | R::NotRegularFile
        | R::Symlink),
    ) = crate::file_resources::resource_failure(error)
    {
        return vec![failure.field_diagnostic(listing, written)];
    }
    let mut issues = crate::file_application::resource_failure(issue::Step::Prepare, error);
    for issue in &mut issues {
        issue.spec_paths = vec![listing.into()];
        issue
            .context
            .insert("path".into(), Value::String(written.into()));
    }
    issues
}

impl Project {
    /// Capture the project file and read every listed specification's name,
    /// links and output files, all through `resolver`: the caller's, or the
    /// local file resolver without one. A project file that cannot be
    /// captured fails with the findings a check of a specification with it
    /// reports.
    pub fn read(
        project: &str,
        host: Option<Language>,
        resolver: Option<Arc<dyn crate::host_resources::Resolver>>,
    ) -> Result<Self, Vec<Diagnostic>> {
        Self::read_located(project, host, resolver).map_err(|(issues, _)| issues)
    }

    /// [`Project::read`], whose refusal keeps the locator of its findings:
    /// the files it read before it stopped (P-ISSUE-1).
    pub fn read_located(
        project: &str,
        host: Option<Language>,
        resolver: Option<Arc<dyn crate::host_resources::Resolver>>,
    ) -> Result<Self, (Vec<Diagnostic>, Locator)> {
        Self::read_to(project, host, resolver, Depth::Listing)
    }

    /// Capture the project file and read each listing to `depth`.
    fn read_to(
        project: &str,
        host: Option<Language>,
        resolver: Option<Arc<dyn crate::host_resources::Resolver>>,
        depth: Depth,
    ) -> Result<Self, (Vec<Diagnostic>, Locator)> {
        let crate::file_workflow::CapturedProject {
            mut resources,
            environment,
        } = crate::file_workflow::capture_project(project, host, resolver).map_err(|error| {
            (
                crate::project_application::workflow_rejection(&error),
                crate::issue_location::workflow(&error),
            )
        })?;
        let identity = environment.root().source().identity.clone();
        let located = || crate::issue_location::project((&environment).into(), [], &Vec::new());
        let declarations = environment
            .project()
            .and_then(|project| project.submission.as_ref().ok())
            .ok_or_else(|| {
                (
                    crate::file_application::rejected(issue::Step::Project, Fault::ProjectSections),
                    located(),
                )
            })?;
        let mut sections = Vec::new();
        let mut written = Vec::new();
        // The sections whose document is a Define-XML 2.0 one (DEFINE-29).
        let mut legacy = Vec::new();
        for section in scope::SECTIONS {
            let Some(declared) = declarations
                .sections
                .iter()
                .find(|declared| declared.family == section)
            else {
                continue;
            };
            sections.push((section, declared.define.is_some()));
            if declared.define.is_some()
                && declared.define_version == yamaa_core::ast::project::DefineVersion::V2_0
            {
                legacy.push(section);
            }
            for (index, spec) in declared.specs.iter().enumerate() {
                written.push((section, index, spec.text.clone()));
            }
        }
        // A project reads the header of each specification it lists at the
        // listing (SCHEMA-2).
        let (listed, documents): (Vec<_>, Vec<_>) = written
            .into_iter()
            .map(|(section, index, written)| {
                let listed = Listed::unread(section, index, written);
                let (mut listed, document) = read_listing(&mut resources, &identity, listed, depth);
                if let Some(document) = document.as_ref().filter(|_| legacy.contains(&section)) {
                    listed.unrepresentable = unrepresentable(document, &listed, section);
                }
                (listed, document)
            })
            .unzip();
        let listings = listed
            .iter()
            .zip(&documents)
            .filter_map(|(listed, document)| Some((listed.listing(), document.as_ref()?)))
            .collect::<Vec<_>>();
        let locator = crate::issue_location::project(
            (&environment).into(),
            listings
                .iter()
                .map(|(listing, document)| (listing.as_str(), *document)),
            &resources.retain_metadata_sources(),
        );
        Ok(Self {
            identity,
            sections,
            listed,
            locator: Arc::new(locator),
        })
    }

    /// What locates the project findings: the project file, the files it
    /// reads and each listed specification that was read (P-ISSUE-1).
    pub fn locator(&self) -> &Arc<Locator> {
        &self.locator
    }

    /// The project file's canonical identity.
    pub fn identity(&self) -> &str {
        &self.identity
    }
    /// Every listing, in section run order and then listing order.
    pub fn listed(&self) -> &[Listed] {
        &self.listed
    }
    /// Declared sections in run order.
    pub fn sections(&self) -> impl Iterator<Item = Submission> + '_ {
        self.sections.iter().map(|(section, _)| *section)
    }
    /// Declared sections that name `define`, in run order.
    pub fn define_sections(&self) -> impl Iterator<Item = Submission> + '_ {
        self.sections
            .iter()
            .filter(|(_, define)| *define)
            .map(|(section, _)| *section)
    }

    /// One finding for each name that two or more listings share.
    pub fn duplicates(&self) -> Vec<Diagnostic> {
        duplicates(&self.identity, &self.listed)
    }

    /// The listings of one section, or of every declared section in run order.
    pub fn scope(&self, section: Option<Submission>) -> Vec<usize> {
        (0..self.listed.len())
            .filter(|&index| section.is_none_or(|section| self.listed[index].section == section))
            .collect()
    }

    /// The project findings a project check adds to its specifications'
    /// findings: shared names, inputs that read a listed artifact without a
    /// link, and files that two listed specifications would both write.
    pub fn check_findings(&self) -> Vec<Diagnostic> {
        let all = self.scope(None);
        let mut issues = self.duplicates();
        for &index in &all {
            issues.extend(self.unlinked(index, &all));
        }
        issues.extend(self.collisions(&all));
        for &index in &all {
            issues.extend(self.listed[index].unrepresentable.iter().cloned());
        }
        issues
    }

    fn unlinked(&self, index: usize, members: &[usize]) -> Vec<Diagnostic> {
        let listed = &self.listed[index];
        let mut issues = Vec::new();
        for input in listed.inputs.iter().filter(|input| input.schema.is_none()) {
            let Some(path) = &input.path else {
                continue;
            };
            if let Some(&producer) = members.iter().find(|&&member| {
                self.listed[member]
                    .artifact_location()
                    .is_some_and(|location| same_location(location, &path.location))
            }) {
                issues.push(identified_in(
                    scope::unlinked_producer(
                        &input.dataset,
                        &path.written,
                        self.listed[producer].identity(),
                    ),
                    listed.identity(),
                    &path.source,
                ));
            }
        }
        issues
    }

    fn collisions(&self, members: &[usize]) -> Vec<Diagnostic> {
        use yamaa_core::specification::ReportFinding;
        let mut seen: Vec<(usize, &'static str, &str)> = Vec::new();
        let mut issues = Vec::new();
        for &member in members {
            let listed = &self.listed[member];
            for (field, located) in [
                ("output.path", &listed.output),
                ("output.report", &listed.report),
            ] {
                let Some(located) = located else {
                    continue;
                };
                // A specification's own two files are its own run's finding.
                if let Some(&(other, left, _)) = seen.iter().find(|(other, _, location)| {
                    *other != member && same_location(location, &located.location)
                }) {
                    let mut finding = identified(
                        ReportFinding::GraphCollision {
                            left,
                            right: field,
                            path: located.written.clone(),
                        }
                        .diagnostic(),
                        listed.identity(),
                    );
                    finding.context.insert(
                        "specifications".into(),
                        Value::Array(vec![
                            Value::String(self.listed[other].identity().into()),
                            Value::String(listed.identity().into()),
                        ]),
                    );
                    issues.push(finding);
                }
                seen.push((member, field, &located.location));
            }
        }
        issues
    }

    /// The run scope of one section, or of every declared section, ordered
    /// producer first, with the findings that fail a specification before it
    /// builds and the project findings that refuse the whole run.
    pub fn plan(&self, section: Option<Submission>) -> Plan {
        let members = self.scope(section);
        let mut refusal = self.duplicates();
        refusal.extend(self.collisions(&members));
        let position = |key: &str| {
            members
                .iter()
                .position(|&member| self.listed[member].key_matches(key))
        };
        let links = members
            .iter()
            .map(|&member| {
                self.listed[member]
                    .inputs
                    .iter()
                    .filter_map(|input| {
                        let schema = input.schema.as_ref()?;
                        Some(Link {
                            dataset: input.dataset.clone(),
                            producer: position(&schema.producer)?,
                            source: schema.source.clone(),
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let dependencies = links
            .iter()
            .map(|links| {
                let mut producers = Vec::new();
                for link in links {
                    if !producers.contains(&link.producer) {
                        producers.push(link.producer);
                    }
                }
                producers
            })
            .collect::<Vec<_>>();
        let order = scope::order(&dependencies);
        let mut refused = members
            .iter()
            .map(|&member| {
                let mut issues = self.listed[member].failure.clone();
                identify(&mut issues, self.listed[member].identity());
                issues.extend(self.unlinked(member, &members));
                issues
            })
            .collect::<Vec<_>>();
        for cycle in &order.cycles {
            let cycle_members = &cycle.members;
            let identities = cycle_members
                .iter()
                .chain(cycle_members.first())
                .map(|&position| self.listed[members[position]].identity())
                .collect::<Vec<_>>();
            for (offset, &member) in cycle_members.iter().enumerate() {
                let next = cycle_members[(offset + 1) % cycle_members.len()];
                let dataset = links[member]
                    .iter()
                    .find(|link| link.producer == next)
                    .map_or("", |link| link.dataset.as_str());
                refused[member].push(identified(
                    scope::workflow_cycle(dataset, &identities),
                    self.listed[members[member]].identity(),
                ));
            }
        }
        let mut refused = refused.into_iter().map(Some).collect::<Vec<_>>();
        let steps = order
            .order
            .into_iter()
            .map(|position| Step {
                listed: members[position],
                producers: links[position]
                    .iter()
                    .map(|link| Link {
                        producer: members[link.producer],
                        ..link.clone()
                    })
                    .collect(),
                refused: refused[position].take().unwrap_or_default(),
            })
            .collect();
        Plan { steps, refusal }
    }
}

/// A project file with the name of every specification its sections list,
/// read only as far as selecting one by its name needs (#4213): a listed
/// specification whose own file writes its `domain` is named without being
/// prepared. Its names and their refusals are those [`Project::read`] gives,
/// located in the project file as it locates them.
pub struct Names(Project);

impl Names {
    /// [`Project::read_located`], reading each listing for its name alone.
    pub fn read_located(
        project: &str,
        host: Option<Language>,
        resolver: Option<Arc<dyn crate::host_resources::Resolver>>,
    ) -> Result<Self, (Vec<Diagnostic>, Locator)> {
        Project::read_to(project, host, resolver, Depth::Name).map(Self)
    }
    /// What locates the project findings; see [`Project::locator`].
    pub fn locator(&self) -> &Arc<Locator> {
        &self.0.locator
    }
    /// See [`Project::duplicates`].
    pub fn duplicates(&self) -> Vec<Diagnostic> {
        self.0.duplicates()
    }
    /// The listing named `name`, or the finding that lists the names the
    /// project defines.
    pub fn find(&self, name: &str) -> Result<&Listed, Vec<Diagnostic>> {
        let listed = &self.0.listed;
        listed
            .iter()
            .find(|listed| listed.name.as_deref() == Some(name))
            .ok_or_else(|| {
                let available = listed
                    .iter()
                    .filter_map(|listed| listed.name.as_deref())
                    .collect::<Vec<_>>();
                let mut finding = scope::unknown_name(name, &available);
                finding
                    .context
                    .insert("source".into(), Value::String(self.0.identity.clone()));
                vec![finding]
            })
    }
}

/// One finding for each name that two or more of `listed` share, in the
/// project file `project`.
fn duplicates(project: &str, listed: &[Listed]) -> Vec<Diagnostic> {
    let mut names = listed
        .iter()
        .filter_map(|listed| listed.name.as_deref())
        .collect::<Vec<_>>();
    names.sort_unstable();
    names.dedup();
    let mut issues = Vec::new();
    for name in names {
        let listings = listed
            .iter()
            .filter(|listed| listed.name.as_deref() == Some(name))
            .map(|listed| (listed.listing(), listed.identity().to_owned()))
            .collect::<Vec<_>>();
        if listings.len() > 1 {
            let mut finding = scope::duplicate_name(name, &listings);
            finding
                .context
                .insert("source".into(), Value::String(project.to_owned()));
            issues.push(finding);
        }
    }
    issues
}

/// How far a project reads each specification it lists.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Depth {
    /// Everything project operations read: the name, links and output files,
    /// from the prepared specification.
    Listing,
    /// Only the name, which selecting a specification by it needs (#4213).
    Name,
}

/// Read the specification `listed` names, unread, to `depth`. A 1.0 project
/// reads its header at the listing first
/// ([`crate::file_preparation::read_header`], SCHEMA-2): a version the engine
/// does not read, or another type, is the listing's own finding, as a listed
/// function or codelist file's is.
///
/// Its name is the `domain` its own file writes, which no parent or fragment
/// replaces (LAYER-3, LAYER-5), or else the `domain` of the prepared
/// specification (PROJECT-5). Read for the name alone, a specification whose
/// own file writes one is not prepared: it has that name whether or not it
/// prepares, since one that cannot be prepared keeps it too (PROJECT-6).
fn read_listing(
    resources: &mut Resources,
    project: &str,
    mut listed: Listed,
    depth: Depth,
) -> (
    Listed,
    Option<crate::specification_source::PreparedDocument>,
) {
    use crate::file_preparation::Header;
    let written = listed.written.clone();
    let written = written.as_str();
    match resources.location_from(project, written) {
        Ok(location) => listed.location = location,
        Err(error) => {
            listed.failure = listing_failure(&listed.listing(), written, &error);
            return (listed, None);
        }
    }
    let mut own = None;
    if let Ok(identity) = resources.resolve_from(project, written) {
        match crate::file_preparation::read_header(
            resources,
            project,
            written,
            &identity,
            &listed.listing(),
        ) {
            Some(Header::Refused(refused)) => {
                listed.identity = Some(identity);
                listed.failure = vec![refused];
                return (listed, None);
            }
            Some(Header::Admitted(model)) => own = Some((identity, domain(&model))),
            None => {}
        }
    }
    if let (Depth::Name, Some((identity, Some(name)))) = (depth, &own) {
        listed.identity = Some(identity.clone());
        listed.name = Some(name.clone());
        return (listed, None);
    }
    let own = own.map(|(_, name)| name);
    let document =
        match crate::file_preparation::prepare_document_from(resources, Some(project), written) {
            Ok(document) => document,
            // A listed file the walk cannot reach is the listing's own finding.
            Err(crate::file_preparation::Error::Resource(
                error @ (ResourceError::Missing
                | ResourceError::NotRegularFile
                | ResourceError::Symlink),
            )) => {
                listed.failure = listing_failure(&listed.listing(), written, &error);
                return (listed, None);
            }
            // A specification that cannot be prepared otherwise, such as one
            // with a validation finding, keeps the name its own file writes,
            // so selecting it reports that finding (PROJECT-6); it has no
            // links here, and its own check or build reports why.
            Err(_) => {
                listed.identity = resources.resolve_from(project, written).ok();
                listed.name = match own {
                    Some(own) => own,
                    None => written_name(resources, project, written),
                };
                return (listed, None);
            }
        };
    let locate = |resources: &Resources, origin: yamaa_core::resource::WrittenPath<'_>| {
        resources
            .location_from(origin.base_source, origin.written)
            .ok()
            .map(|location| Located {
                location,
                written: origin.written.to_owned(),
                source: origin.declaring_source.to_owned(),
            })
    };
    let model = document.document();
    listed.identity = Some(document.source().identity.clone());
    listed.name = own.flatten().or_else(|| domain(model));
    if let Some(N::Mapping(inputs)) = model
        .field(model.root(), "input")
        .map(|input| &model.nodes()[input])
    {
        for &(name, _) in inputs {
            let Some(dataset) = text(model, name) else {
                continue;
            };
            let path = document
                .written_input_origin(dataset, "path")
                .and_then(|origin| locate(resources, origin));
            let schema = document
                .written_input_origin(dataset, "schema")
                .and_then(|origin| {
                    resources
                        .resolve_from(origin.base_source, origin.written)
                        .or_else(|_| resources.location_from(origin.base_source, origin.written))
                        .ok()
                        .map(|producer| Schema {
                            producer,
                            source: origin.declaring_source.to_owned(),
                        })
                });
            listed.inputs.push(Input {
                dataset: dataset.to_owned(),
                path,
                schema,
            });
        }
    }
    listed.output = document
        .written_output_field_origin("path")
        .and_then(|origin| locate(resources, origin));
    listed.report = document
        .written_output_field_origin("report")
        .and_then(|origin| locate(resources, origin));
    (listed, Some(document))
}

/// The origins `document` declares that its section's Define-XML 2.0
/// document cannot record, each named as the listing's and located in the
/// layer that wrote it (P-ISSUE-1): the refusal composition makes, found
/// without composing (PROJECT-7).
fn unrepresentable(
    document: &crate::specification_source::PreparedDocument,
    listed: &Listed,
    section: Submission,
) -> Vec<Diagnostic> {
    yamaa_core::define_legacy::origin_refusals(document.tree(), section)
        .into_iter()
        .map(|refusal| {
            let source = refusal
                .spec_paths
                .first()
                .and_then(|path| document.declaring_source(path))
                .unwrap_or(listed.identity())
                .to_owned();
            identified_in(refusal, listed.identity(), &source)
        })
        .collect()
}

/// The `domain` the listed file itself writes, read without preparing it: the
/// name of a listing whose specification cannot be prepared. A `domain` only
/// a parent layer supplies is not read, so such a listing has no name.
fn written_name(resources: &mut Resources, project: &str, written: &str) -> Option<String> {
    let (bytes, _) = resources.capture_metadata_from(project, written).ok()?;
    domain(&crate::yaml_decode::decode_yaml(&bytes).ok()?.document)
}

/// The `domain` text `model` writes at its root.
fn domain(model: &Document) -> Option<String> {
    let node = model.field(model.root(), "domain")?;
    text(model, node).map(str::to_owned)
}

/// One specification of a run scope, in run order.
#[derive(Clone, Debug)]
pub struct Step {
    listed: usize,
    /// In-scope producer links, each naming its producer by listing.
    producers: Vec<Link>,
    refused: Vec<Diagnostic>,
}
impl Step {
    /// The listing this step runs.
    pub fn listed(&self) -> usize {
        self.listed
    }
    /// Findings that fail the specification before it builds.
    pub fn refused(&self) -> &[Diagnostic] {
        &self.refused
    }
    /// The run-scope producers this specification reads, by listing.
    pub fn producers(&self) -> impl Iterator<Item = usize> + '_ {
        self.producers.iter().map(|link| link.producer)
    }
    /// Each in-scope producer link: the producer's listing and the
    /// `producer_not_completed` finding the specification reports when that
    /// producer did not complete, naming the consumer as its entry and the
    /// layer that wrote the link as its source.
    pub fn links(&self, project: &Project) -> Vec<(usize, Diagnostic)> {
        let consumer = project.listed[self.listed].identity();
        self.producers
            .iter()
            .map(|link| {
                let finding = scope::producer_not_completed(
                    &link.dataset,
                    project.listed[link.producer].identity(),
                );
                (
                    link.producer,
                    identified_in(finding, consumer, &link.source),
                )
            })
            .collect()
    }
    /// One `producer_not_completed` finding for each link whose producer
    /// `failed` reports, naming the consumer as their entry.
    pub fn blocked(&self, project: &Project, failed: impl Fn(usize) -> bool) -> Vec<Diagnostic> {
        self.links(project)
            .into_iter()
            .filter(|(producer, _)| failed(*producer))
            .map(|(_, finding)| finding)
            .collect()
    }
}

/// The producer-first steps of one run scope and the project findings that
/// refuse the run before any specification builds.
#[derive(Clone, Debug)]
pub struct Plan {
    steps: Vec<Step>,
    refusal: Vec<Diagnostic>,
}
impl Plan {
    pub fn steps(&self) -> &[Step] {
        &self.steps
    }
    pub fn refusal(&self) -> &[Diagnostic] {
        &self.refusal
    }
}

/// Stack run reports in the given order into one table with the run report's
/// columns; no report gives the columns without rows.
pub fn stack_reports(reports: &[&PublicTable]) -> Result<PublicTable, Vec<Diagnostic>> {
    let refused = || crate::file_application::rejected(issue::Step::Report, Fault::ReportStack);
    if reports.is_empty() {
        return crate::run_report::empty_table().map_err(|_| refused());
    }
    PublicTable::stack(reports).map_err(|_| refused())
}
