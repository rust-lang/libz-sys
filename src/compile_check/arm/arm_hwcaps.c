#include <sys/auxv.h>
#include <asm/hwcap.h>
int main() {
    return (getauxval(AT_HWCAP2) & HWCAP2_CRC32);
}