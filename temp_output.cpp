#include "pico/stdlib.h"

int main() {
    stdio_init_all();
    gpio_init(16);
        gpio_set_dir(16, GPIO_OUT);
        gpio_put(16, 1);
    };
    while(1) { }
    return 0;
}