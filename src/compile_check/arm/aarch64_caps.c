#include <sys/auxv.h>
int main() {
    return (getauxval(AT_HWCAP2) & HWCAP2_CRC32);
}
