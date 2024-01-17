#include <immintrin.h>
__mmask16 f(__mmask16 x) { return _knot_mask16(x); }
int main(void) { return 0; }