#include "pico/stdlib.h"
#include "hardware/flash.h"
#include "hardware/watchdog.h"

int main() {
	stdio_init_all();
		gpio_init(16);
        gpio_set_dir(16, GPIO_OUT);
        gpio_put(16, 1);
    };
		sleep_500(500);
    };
	return 0;
}