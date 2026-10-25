/* The memcheck control of tools/commands/check_r.py --valgrind, which
 * compiles and runs this program under valgrind with the options of the
 * package's check before that check. It loses three blocks:
 *
 * - one at a root that memcheck.supp lists, reached only by a pointer into
 *   its middle (possibly lost), which the suppression must hide;
 * - one at that same root that nothing reaches (definitely lost), which
 *   memcheck must still count, since each suppression covers only the
 *   possibly lost kind;
 * - one at another root, reached only by a pointer into its middle, which
 *   memcheck must still count.
 *
 * So memcheck must report 2 errors and 1 suppressed. The listed root's name
 * matches memcheck.supp's pattern for extendr's ownership map.
 */
#include <stdlib.h>

static char *middle[2];

__attribute__((noinline)) void extendr_api_ownership_OWNERSHIP_control(void) {
    char *possible = malloc(4096);
    middle[0] = possible + 64;
    char *definite = malloc(1024);
    definite[0] = 1;
    definite = NULL;
}

__attribute__((noinline)) void unrelated_possible_loss(void) {
    char *possible = malloc(2048);
    middle[1] = possible + 64;
}

int main(void) {
    extendr_api_ownership_OWNERSHIP_control();
    unrelated_possible_loss();
    return middle[0] == NULL || middle[1] == NULL;
}
