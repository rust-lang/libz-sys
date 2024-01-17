#include <immintrin.h>
#include <wmmintrin.h>
__m512i f(__m512i a) {
    __m512i b = _mm512_setzero_si512();
    return _mm512_clmulepi64_epi128(a, b, 0x10);
}
int main(void) { return 0; }
