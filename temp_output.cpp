#include "pico/stdlib.h"
#include "hardware/flash.h"
#include "hardware/watchdog.h"

int main() {
	stdio_init_all();
	while(true) {
		sleep_ms(500);
	}
	return 0;
}
