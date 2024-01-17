#include <immintrin.h>
__m512i f(__m512i x, __m512i y) {
    __m512i z = _mm512_setzero_epi32();
    return _mm512_dpbusd_epi32(z, x, y);
}
int main(void) { return 0; }
