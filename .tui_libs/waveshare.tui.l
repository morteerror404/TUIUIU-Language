funcao piscar(pino) {
    NATIVO {
        gpio_init(pino);
        gpio_set_dir(pino, GPIO_OUT);
        gpio_put(pino, 1);
    }
}
funcao esperar(ms) {
    NATIVO {
        sleep_ms(ms);
    }
}