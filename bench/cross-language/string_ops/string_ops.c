#include <stdio.h>
#include <stdlib.h>
#include <string.h>

int main() {
    int count = 100000;
    char* s = (char*)malloc(count + 1);
    memset(s, 'x', count);
    s[count] = '\0';
    printf("Length: %d\n", (int)strlen(s));
    free(s);
    return 0;
}
