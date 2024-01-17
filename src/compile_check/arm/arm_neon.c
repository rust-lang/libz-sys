#include <sys/auxv.h>
int main() {
    return (getauxval(AT_HWCAP) & HWCAP_ARM_NEON);
}
