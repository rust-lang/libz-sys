// Check whether compiler supports loading 4 neon vecs into a register range
#if defined(_MSC_VER) && (defined(_M_ARM64) || defined(_M_ARM64EC))
    #include <arm64_neon.h>
#else
    #include <arm_neon.h>
#endif
int32x4x4_t f(int var[16]) { return vld1q_s32_x4(var); }
int main(void) { return 0; }
