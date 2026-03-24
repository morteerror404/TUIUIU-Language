void set_rgb(uint pin, uint8_t r, uint8_t g, uint8_t b) {
    for (int i = 0; i < 24; i++) {
        uint32_t color = ((uint32_t)g << 16) | ((uint32_t)r << 8) | b;
        if (color & (1 << (23 - i))) {
            gpio_put(pin, 1); sleep_us(0.8); gpio_put(pin, 0); sleep_us(0.45);
        } else {
            gpio_put(pin, 1); sleep_us(0.4); gpio_put(pin, 0); sleep_us(0.85);
        }
    }
    sleep_us(80); // Reset pulse
}
