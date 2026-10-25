/* The memcheck control of tools/commands/check_r.py --valgrind, which
 * compiles and runs this program under valgrind with the options of the
 * package's check before that check. Each function is named so that
 * memcheck.supp's patterns match it as they match the Rust function of the
 * same path. It loses seven blocks, each reached only by a pointer into its
 * middle (possibly lost) unless stated:
 *
 * - at extendr's ownership map: one, which the suppression must hide, and
 *   one that nothing reaches (definitely lost), which must still count,
 *   since each suppression covers only the possibly lost kind;
 * - in std's current-thread handle: one asked for by the R binding's
 *   cancellation token, hidden, and one asked for by another caller,
 *   counted;
 * - in a regex's lazy DFA cache: one searched by a schema descriptor's
 *   constraint check, hidden, and one searched by another caller, counted;
 * - one at an unrelated root, counted.
 *
 * So memcheck must report 4 errors and 3 suppressed.
 */
#include <stdlib.h>

static char *middle[6];
static int next;

static void lose_middle(size_t size) {
    char *block = malloc(size);
    middle[next++] = block + 64;
}

__attribute__((noinline)) void extendr_api_ownership_OWNERSHIP_control(void) {
    lose_middle(4096);
    char *definite = malloc(1024);
    definite[0] = 1;
    definite = NULL;
}

__attribute__((noinline)) void std_thread_current_init_current_control(void) {
    lose_middle(256);
}

__attribute__((noinline)) void yamaa_r_interrupt_token_control(void) {
    std_thread_current_init_current_control();
}

__attribute__((noinline)) void another_thread_caller(void) {
    std_thread_current_init_current_control();
}

__attribute__((noinline)) void regex_automata_hybrid_dfa_Lazy_control(void) {
    lose_middle(512);
}

__attribute__((noinline)) void yamaa_core_schema_descriptor_Descriptor_check_constraints_control(void) {
    regex_automata_hybrid_dfa_Lazy_control();
}

__attribute__((noinline)) void another_regex_caller(void) {
    regex_automata_hybrid_dfa_Lazy_control();
}

__attribute__((noinline)) void unrelated_possible_loss(void) {
    lose_middle(2048);
}

int main(void) {
    extendr_api_ownership_OWNERSHIP_control();
    yamaa_r_interrupt_token_control();
    another_thread_caller();
    yamaa_core_schema_descriptor_Descriptor_check_constraints_control();
    another_regex_caller();
    unrelated_possible_loss();
    return next != 6;
}
