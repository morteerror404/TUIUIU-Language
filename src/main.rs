use clap::{Parser, Subcommand};
use std::collections::HashMap;
use std::fs;
use std::process::Command;
use std::time::{Duration, Instant};
use std::path::Path;
#[allow(dead_code, unused_imports)]
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
        #[arg(short, long)]
        drive: Option<String>,
    },
    Clean,
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
    #[allow(dead_code)]
    fn verificar_ferramentas(&self) {
        let ferramentas = ["cmake", "arm-none-eabi-gcc", "ninja"];
        for tool in ferramentas {
            match Command::new(tool).arg("--version").output() {
                Ok(_) => println!("OK: {} encontrado.", tool),
                Err(_) => eprintln!("AVISO: {} nao encontrado! O build pode falhar.", tool),
            }
        }
    }

    fn limpar_cache(&self) {
    println!("Limpando ambiente de build...");
    if Path::new("build").exists() {
        let _ = fs::remove_dir_all("build");
        println!("Pasta 'build' removida.");
    }
    if Path::new("temp_output.cpp").exists() {
        let _ = fs::remove_file("temp_output.cpp");
    }
    if Path::new("CMakeLists.txt").exists() {
        let _ = fs::remove_file("CMakeLists.txt");
    }
    println!("Ambiente limpo.");
}

    fn flash_dispositivo(&self, drive: &str) {
    let drive_limpo = drive.trim_end_matches(':').to_uppercase();
    let origem = "build/firmware_tui.uf2";
    let destino = format!("{}:/firmware.uf2", drive_limpo);

    println!("Copiando para a unidade {}:...", drive_limpo);

    match fs::copy(origem, &destino) {
        Ok(_) => println!("Flash concluído com sucesso! O Pico deve reiniciar agora."),
        Err(e) => {
            eprintln!("Erro ao copiar para {}: {}. O Pico esta em modo BOOTSEL?", destino, e);
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
    // Comandos nativos
    self.biblioteca.insert("esperar".to_string(), "sleep_ms(ms)".to_string());    
    self.biblioteca.insert("ligar".to_string(), "gpio_init(pino); gpio_set_dir(pino, GPIO_OUT); gpio_put(pino, 1)".to_string());
    self.biblioteca.insert("desligar".to_string(), "gpio_put(pino, 0)".to_string());

    // Carrega extensões da pasta .tui_libs
    if let Ok(entradas) = fs::read_dir(".tui_libs") {
        for entrada in entradas.flatten() {
            if let Ok(conteudo) = fs::read_to_string(entrada.path()) {
                for linha in conteudo.lines() {
                    let partes: Vec<&str> = linha.split(':').collect();
                    if partes.len() == 2 {
                        self.biblioteca.insert(partes[0].trim().to_string(), partes[1].trim().to_string());
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
    let ninja_path = "C:/Program Files/CMake/bin/ninja.exe";

    let mut sdk_path = String::new();
    let caminhos_possiveis = [
        "C:/Program Files/Raspberry Pi/Pico SDK v1.5.1/pico-sdk",
        "C:/Program Files/Raspberry Pi/Pico SDK v1.5.1",
        "C:/pico-sdk",
        "D:/pico-sdk",
    ];

    for caminho in caminhos_possiveis {
        if Path::new(caminho).join("pico_sdk_version.cmake").exists() {
            sdk_path = caminho.to_string();
            break;
        }
    }

    if sdk_path.is_empty() {
        eprintln!("Erro: Não foi possível encontrar o Pico SDK.");
        return;
    }

    let cmake_content = format!(
        "cmake_minimum_required(VERSION 3.13)\n\
         set(PICO_NO_PICOTOOL 1)\n\
         include({})\n\
         project(TuiuiuProject C CXX ASM)\n\
         pico_sdk_init()\n\
         add_executable(firmware_tui temp_output.cpp)\n\
         pico_add_extra_outputs(firmware_tui)\n\
         target_link_libraries(firmware_tui pico_stdlib hardware_flash hardware_watchdog)\n",
        pico_import_file
    );
    fs::write("CMakeLists.txt", cmake_content).ok();

    if !Path::new(build_dir).exists() { fs::create_dir(build_dir).ok(); }

    println!("Configurando CMake...");
    let _ = Command::new("cmake")
        .arg("-S").arg(".")
        .arg("-B").arg(build_dir)
        .arg("-G").arg("Ninja")
        .arg(format!("-DCMAKE_MAKE_PROGRAM={}", ninja_path))
        .arg(format!("-DPICO_SDK_PATH={}", sdk_path.replace("\\", "/")))
        .status();

    println!("Iniciando compilação...");
    let _ = Command::new("cmake").arg("--build").arg(build_dir).status();

    let uf2_path = format!("{}/firmware_tui.uf2", build_dir);
    let elf_path = format!("{}/firmware_tui.elf", build_dir);

    // Conversão manual se necessário
    if !Path::new(&uf2_path).exists() && Path::new(&elf_path).exists() {
        let _ = Command::new(".\\elf2uf2.exe").arg(&elf_path).arg(&uf2_path).status();
    }

    if Path::new(&uf2_path).exists() {
        println!("Sucesso: firmware_tui.uf2 gerado.");

        if let Some(letra) = drive_letra {
            let letra_limpa = letra.trim_end_matches(':').to_uppercase(); // Remove : extra
            println!("\n--> Deseja enviar para a unidade {}: agora? (s/n)", letra_limpa);
            
            let mut input = String::new();
            std::io::stdin().read_line(&mut input).ok();

            if input.trim().to_lowercase() == "s" {
                self.flash_dispositivo(&letra_limpa);
            } else {
                // SALVAR NA RAIZ SE DISSER "N"
                let destino_raiz = "firmware_tui.uf2";
                match fs::copy(&uf2_path, destino_raiz) {
                    Ok(_) => println!("Arquivo salvo na raiz: {}", destino_raiz),
                    Err(e) => eprintln!("Erro ao salvar na raiz: {}", e),
                }
            }
        }
    } else {
        eprintln!("Erro Crítico: UF2 não encontrado.");
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

    fn transpilador(&mut self, arquivo_tui: &str, _force_hardened: bool, drive: Option<String>) {
        let conteudo = fs::read_to_string(arquivo_tui).unwrap_or_else(|_| {
            eprintln!("Erro: Nao foi possivel ler o arquivo {}", arquivo_tui);
            String::new()
        });
        
        if conteudo.is_empty() { return; }

        // 1. Identificação da placa e carregamento de mapeamentos
        let mut placa_nome = String::new();
        for linha in conteudo.lines() {
            if linha.trim().starts_with("import ") {
                placa_nome = linha.split_whitespace().last().unwrap_or("").replace("\"", "");
            }
        }

        if placa_nome.is_empty() { 
            eprintln!("Erro: Nenhuma placa importada (ex: import \"waveshare_rp2040_zero\")");
            return; 
        }

        // Carrega o .tui.m e as bibliotecas
        let _ = self.carregar_mapeamento_fabrica(&placa_nome);
        self.carregar_bibliotecas();
        self.processar_bloco_hardware(&conteudo);

        // 2. Cabeçalho C++ com bibliotecas do Pico SDK
        let mut cpp_output = String::new();
        cpp_output.push_str("#include \"pico/stdlib.h\"\n#include \"hardware/flash.h\"\n#include \"hardware/watchdog.h\"\n\n");
        cpp_output.push_str("int main() {\n\tstdio_init_all();\n");

        // 3. Inicialização Automática de Pins (Reconhecimento de Hardware)
        // O compilador já sabe quais pins usar e configura-os no setup do C++
        for (alias, pino_num) in &self.hardware_local {
            cpp_output.push_str(&format!("\tgpio_init({});\n", pino_num));
            cpp_output.push_str(&format!("\tgpio_set_dir({}, GPIO_OUT);\n", pino_num));
        }

        // 4. Processamento de Linhas de Código
        let mut dentro_bloco_hardware = false;

        for linha in conteudo.lines() {
            let l = linha.trim();
            
            // Ignora o bloco .hardware[] na tradução de comandos (já processado acima)
            if l.starts_with(".hardware[") { dentro_bloco_hardware = true; continue; }
            if l == "]" && dentro_bloco_hardware { dentro_bloco_hardware = false; continue; }
            
            // Pula linhas de metadados, comentários e definições internas
            if l.is_empty() || dentro_bloco_hardware || l.starts_with('@') || l.starts_with("import") || l.starts_with("//") {
                continue;
            }

            if l.contains("repetir {") {
                cpp_output.push_str("\twhile(true) {\n");
            } else if l == "}" {
                cpp_output.push_str("\t}\n"); 
            } else {
                let mut comando_encontrado = false;
                for (func_tui, snip_c) in &self.biblioteca {
                    if l.starts_with(func_tui) {
                        let mut comando_final = snip_c.clone();
                        
                        // Substitui argumentos de tempo: esperar(500) -> sleep_ms(500)
                        if let (Some(start), Some(end)) = (l.find('('), l.find(')')) {
                            let arg_valor = &l[start + 1..end];
                            comando_final = comando_final.replace("(ms)", &format!("({})", arg_valor));
                        }
                        
                        // SUBSTITUIÇÃO DE HARDWARE: Troca o alias pelo número real do pino
                        // Ex: status_led vira 16 (Waveshare RP2040-Zero)
                        for (alias, pino_num) in &self.hardware_local {
                            if l.contains(alias) {
                                comando_final = comando_final.replace("pino", &pino_num.to_string());
                            }
                        }

                        let cmd_limpo = comando_final.trim_end_matches(';').to_string() + ";";
                        
                        // Formatação de indentação para o loop
                        if cpp_output.contains("while(true) {") {
                            cpp_output.push_str(&format!("\t\t{}\n", cmd_limpo));
                        } else {
                            cpp_output.push_str(&format!("\t{}\n", cmd_limpo));
                        }
                        
                        comando_encontrado = true;
                        break; 
                    }
                }
                
                if !comando_encontrado {
                    println!("Aviso: O comando '{}' não foi mapeado ou reconhecido.", l);
                }
            }
        }

        cpp_output.push_str("\treturn 0;\n}\n");

        // 5. Salva e chama a compilação
        if fs::write("temp_output.cpp", &cpp_output).is_ok() {
            println!("Transpilação concluída: temp_output.cpp gerado com sucesso.");
            self.compilar_para_uf2(drive);
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
        Commands::Clean => {
            // Chama a funcao de limpeza que voce ja tem no código
            compiler.limpar_cache();
        }
        Commands::Install { nome } => {
            let _ = fs::create_dir_all(".tui_mapping");
            let _ = fs::create_dir_all(".tui_libs");
            
            // Formatamos a biblioteca para o padrao nome:comando
            let lib_content = "piscar:gpio_init(pino); gpio_set_dir(pino, GPIO_OUT); gpio_put(pino, 1); sleep_ms(500); gpio_put(pino, 0)";
            
            fs::write(format!(".tui_mapping/{}.tui.m", nome), "@LED: 25\n@PANIC_PIN: 0").ok();
            fs::write(format!(".tui_libs/{}.tui.l", nome), lib_content).ok();
            
            println!("Recursos para '{}' instalados.", nome);
        }
        Commands::Build { arquivo, hardened, drive } => {
            compiler.transpilador(arquivo, *hardened, drive.clone());
        }
    }
}