#include <immintrin.h>
__m512i f(__m512i y) {
__m512i x = _mm512_set1_epi8(2);
    return _mm512_sub_epi8(x, y);
}
int main(void) { return 0; }
