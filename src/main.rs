use clap::{Parser, Subcommand};
use std::collections::HashMap;
use std::fs;
use std::process::Command;
use std::time::{Duration, Instant};
use std::path::Path;

#[derive(Parser)]
#[command(name = "tui")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Init,
    #[command(short_flag = 'i')]
    Install { nome: String },
    Build { 
        arquivo: String,
        #[arg(long)]
        hardened: bool,
        /// Unidade de disco do Pico (ex: E, G, F)
        #[arg(short, long)]
        drive: Option<String>,
    },
}

struct TuiCompiler {
    hardware_factory: HashMap<String, u8>,
    hardware_local: HashMap<String, u8>,
    biblioteca: HashMap<String, String>,
}

impl TuiCompiler {
    fn new() -> Self {
        Self {
            hardware_factory: HashMap::new(),
            hardware_local: HashMap::new(),
            biblioteca: HashMap::new(),
        }
    }

    fn flash_dispositivo(&self, letra: &str) {
        let origem = "build/firmware_tui.uf2";
        let destino = format!("{}:\\firmware_tui.uf2", letra.to_uppercase());

        if Path::new(origem).exists() {
            println!("Tentando flash na unidade {}:...", letra);
            // Tenta copiar o arquivo. Se falhar, o Pico provavelmente não está em modo BOOTSEL.
            if fs::copy(origem, &destino).is_ok() {
                println!("Flash concluido! O dispositivo deve reiniciar agora.");
            } else {
                eprintln!("Erro: Nao foi possivel copiar para {}. O Pico esta em modo BOOTSEL?", letra);
            }
        }
    }

    fn inicializar_ambiente(&self) -> bool {
        println!("Verificando e reparando ambiente Tuiuiu...");
        let mut sucesso = true;

        let pastas = [".tui_mapping", ".tui_libs", "build"];
        for pasta in pastas {
            if !Path::new(pasta).exists() {
                if fs::create_dir(pasta).is_err() { sucesso = false; }
            }
        }

        let pico_import = "pico_sdk_import.cmake";
        if !Path::new(pico_import).exists() {
            println!("Dependencia ausente: Baixando pico_sdk_import.cmake...");
            let url = "https://raw.githubusercontent.com/raspberrypi/pico-sdk/master/external/pico_sdk_import.cmake";
            
            let download = Command::new("powershell")
                .arg("-Command")
                .arg(format!("Invoke-WebRequest -Uri {} -OutFile {}", url, pico_import))
                .status();

            if download.is_err() || !download.unwrap().success() {
                eprintln!("Erro: Falha ao baixar dependencia via PowerShell.");
                sucesso = false;
            }
        }

        let gcc_check = Command::new("arm-none-eabi-gcc").arg("--version").output();
        if gcc_check.is_err() {
            eprintln!("Aviso: Toolchain ARM nao detectada no sistema.");
        }

        sucesso
    }

    fn carregar_mapeamento_fabrica(&mut self, nome_placa: &str) -> Result<(), String> {
        let caminho = format!(".tui_mapping/{}.tui.m", nome_placa);
        if !Path::new(&caminho).exists() {
            return Err(format!("Placa '{}' nao instalada. Rode 'tui -i {}'", nome_placa, nome_placa));
        }

        let conteudo = fs::read_to_string(&caminho).unwrap_or_default();
        for (idx, linha) in conteudo.lines().enumerate() {
            let linha = linha.trim().trim_start_matches('\u{feff}');
            if linha.is_empty() || linha.starts_with("//") { continue; }
            if linha.starts_with('@') {
                let partes: Vec<&str> = linha.split(':').collect();
                if partes.len() == 2 {
                    let nome = partes[0].trim().trim_start_matches('@').to_string();
                    let pino = partes[1].trim().parse::<u8>()
                        .map_err(|_| format!("Erro no Mapa [Linha {}]: Pino '{}' invalido.", idx + 1, partes[1]))?;
                    self.hardware_factory.insert(nome, pino);
                }
            }
        }
        Ok(())
    }

    fn carregar_bibliotecas(&mut self) {
        if let Ok(entries) = fs::read_dir(".tui_libs") {
            for entry in entries.flatten() {
                if let Ok(conteudo) = fs::read_to_string(entry.path()) {
                    let blocos = conteudo.split("funcao ");
                    for bloco in blocos {
                        if bloco.trim().is_empty() { continue; }
                        if let Some(inicio_n) = bloco.find("NATIVO {") {
                            if let Some(fim_n) = bloco.rfind('}') {
                                let f_name = bloco.split('(').next().unwrap_or("").trim();
                                let codigo_c = &bloco[inicio_n + 8..fim_n].trim();
                                self.biblioteca.insert(f_name.to_string(), codigo_c.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    fn processar_bloco_hardware(&mut self, conteudo: &str) {
        if let Some(start_idx) = conteudo.find(".hardware[") {
            let rest = &conteudo[start_idx..];
            if let Some(end_idx) = rest.find(']') {
                let bloco = &rest[10..end_idx];
                for linha in bloco.lines() {
                    let l = linha.trim();
                    if l.contains('=') {
                        let partes: Vec<&str> = l.split('=').collect();
                        let alias = partes[0].trim().to_string();
                        let hw_ref = partes[1].trim().trim_start_matches('@').to_string();
                        if let Some(&pino) = self.hardware_factory.get(&hw_ref) {
                            self.hardware_local.insert(alias, pino);
                        }
                    }
                }
            }
        }
    }

    fn compilar_para_uf2(&self, drive_letra: Option<String>) {
        let build_dir = "build";
        let pico_import_file = "pico_sdk_import.cmake";

        if !Path::new(pico_import_file).exists() {
            if !self.inicializar_ambiente() { return; }
        }

        // 1. Busca do SDK (Prioridade para o que existe no disco)
        let mut sdk_path = String::new();
        let locais_comuns = [
            "C:\\pico-sdk",
            "D:\\pico-sdk",
            &format!("{}\\pico-sdk", std::env::var("USERPROFILE").unwrap_or_default()),
        ];

        for local in locais_comuns {
            if Path::new(local).join("pico_sdk_version.cmake").exists() {
                sdk_path = local.to_string();
                break;
            }
        }

        if sdk_path.is_empty() {
            sdk_path = std::env::var("PICO_SDK_PATH").unwrap_or_default();
        }

        if sdk_path.is_empty() || !Path::new(&sdk_path).exists() {
            eprintln!("Erro: Pico SDK nao encontrado fisicamente.");
            return;
        }

        // 2. Gerar o CMakeLists.txt
        let cmake_content = format!(
            "cmake_minimum_required(VERSION 3.13)\n\
             include({})\n\
             project(TuiuiuProject C CXX ASM)\n\
             pico_sdk_init()\n\
             add_executable(firmware_tui temp_output.cpp)\n\
             target_link_libraries(firmware_tui pico_stdlib hardware_flash hardware_watchdog)\n\
             pico_add_extra_outputs(firmware_tui)",
            pico_import_file
        );
        fs::write("CMakeLists.txt", cmake_content).ok();

        if !Path::new(build_dir).exists() { fs::create_dir(build_dir).ok(); }

        println!("Configurando CMake forçando SDK em: {}", sdk_path);
        
        // 3. O PULO DO GATO: Passar o caminho via -D para o CMake ignorar o ambiente do Windows
        let cmake_status = Command::new("cmake")
            .arg("-S").arg(".")
            .arg("-B").arg(build_dir)
            .arg("-G").arg("MinGW Makefiles")
            .arg(format!("-DPICO_SDK_PATH={}", sdk_path.replace("\\", "/"))) // Força o caminho correto
            .status();

        if let Ok(status) = cmake_status {
            if status.success() {
                println!("Compilando...");
                let build_status = Command::new("cmake")
                    .arg("--build").arg(build_dir)
                    .status();

                if let Ok(b_status) = build_status {
                    if b_status.success() {
                        println!("Sucesso: firmware_tui.uf2 gerado.");
                        if let Some(letra) = drive_letra {
                            self.flash_dispositivo(&letra);
                        }
                    }
                }
            } else {
                eprintln!("Erro: O CMake falhou. Verifique se o compilador ARM (gcc-arm-none-eabi) esta instalado.");
            }
        }
    }

    fn gerar_codigo_estresse(&self) -> String {
        let mut c = String::from("#include \"pico/stdlib.h\"\nint main() {\n\tstdio_init_all();\n");
        for (nome, pino) in &self.hardware_factory {
            c.push_str(&format!(
                "\t// Estresse em {}\n\tgpio_init({}); gpio_set_dir({}, GPIO_OUT);\n\tgpio_put({}, 1); sleep_ms(100);\n", 
                nome, pino, pino, pino
            ));
        }
        c.push_str("\twhile(1) { tight_loop_contents(); }\n\treturn 0;\n}");
        c
    }

    fn transpilador(&mut self, arquivo_tui: &str, force_hardened: bool, drive: Option<String>) {
        let conteudo = match fs::read_to_string(arquivo_tui) {
            Ok(c) => c,
            Err(_) => {
                eprintln!("Erro: Arquivo '{}' nao encontrado.", arquivo_tui);
                return;
            }
        };

        let mut placa_nome = String::new();
        let mut logic_found = false;

        for linha in conteudo.lines() {
            let l = linha.trim();
            if l.starts_with("import ") {
                placa_nome = l.split_whitespace().last().unwrap_or("").replace("\"", "");
            } else if !l.is_empty() && !l.starts_with('.') && !l.starts_with('@') && !l.starts_with("//") {
                logic_found = true;
            }
        }

        if placa_nome.is_empty() {
            eprintln!("Erro: 'import [placa]' ausente no arquivo.");
            return;
        }

        if let Err(e) = self.carregar_mapeamento_fabrica(&placa_nome) {
            eprintln!("{}", e);
            return;
        }

        self.carregar_bibliotecas();
        self.processar_bloco_hardware(&conteudo);

        let tempo_inicio = Instant::now();
        let mut cpp_output = String::new();

        if !logic_found {
            println!("Vazio detectado. Redirecionando para Firmware de Estresse.");
            cpp_output = self.gerar_codigo_estresse();
        } else {
            cpp_output.push_str("#include \"pico/stdlib.h\"\n#include \"hardware/flash.h\"\n#include \"hardware/watchdog.h\"\n\nint main() {\n\tstdio_init_all();\n");
            
            if force_hardened {
                let pino_panic = self.hardware_factory.get("PANIC_PIN").unwrap_or(&0);
                cpp_output.push_str(&format!(
                    "\tgpio_init({}); gpio_set_dir({}, GPIO_IN);\n\tif(gpio_get({})) {{ flash_range_erase(0, FLASH_TARGET_CONTENTS_SIZE); watchdog_reboot(0,0,0); }}\n", 
                    pino_panic, pino_panic, pino_panic
                ));
            }

            for linha in conteudo.lines() {
                let l = linha.trim();
                if l.is_empty() || l.starts_with("import") || l.starts_with('.') || l.starts_with("//") { 
                    if l.starts_with("repetir {") { cpp_output.push_str("\twhile(1) {\n"); }
                    if l == "}" { cpp_output.push_str("\t}\n"); }
                    continue; 
                }

                for (func, snip_c) in &self.biblioteca {
                    if l.contains(func) {
                        let mut final_c = snip_c.clone();
                        for (alias, pino) in &self.hardware_local {
                            if l.contains(alias) { final_c = final_c.replace("pino", &pino.to_string()); }
                        }
                        if let Some(s) = l.find('(') {
                            let arg = &l[s+1..l.find(')').unwrap_or(s+1)];
                            final_c = final_c.replace("ms", arg);
                        }
                        cpp_output.push_str(&format!("\t\t{};\n", final_c));
                    }
                }
            }
            cpp_output.push_str("\treturn 0;\n}");
        }

        fs::write("temp_output.cpp", cpp_output).ok();
        self.compilar_para_uf2(drive);

        let total = tempo_inicio.elapsed();
        if total >= Duration::from_secs(5) {
            println!("ALERTA CRITICO: Tempo de resposta do sistema/hardware excede 5s ({:?}).", total);
        } else {
            println!("Operacao concluida com sucesso em {:?}.", total);
        }
    }
}

fn main() {
    let cli = Cli::parse();
    let mut compiler = TuiCompiler::new();

    match &cli.command {
        Commands::Init => {
            compiler.inicializar_ambiente();
        }
        Commands::Install { nome } => {
            let _ = fs::create_dir_all(".tui_mapping");
            let _ = fs::create_dir_all(".tui_libs");
            fs::write(format!(".tui_mapping/{}.tui.m", nome), "@LED_RGB: 16\n@PANIC_PIN: 0").ok();
            fs::write(format!(".tui_libs/{}.tui.l", nome), "funcao piscar(pino) { NATIVO { gpio_init(pino); gpio_set_dir(pino, GPIO_OUT); gpio_put(pino, 1); } }\nfuncao esperar(ms) { NATIVO { sleep_ms(ms); } }").ok();
            println!("Recursos para '{}' instalados.", nome);
        }
        Commands::Build { arquivo, hardened, drive   } => {
            compiler.transpilador(arquivo, *hardened, drive.clone());
        }
    }
}