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

    fn flash_dispositivo(&self, letra: &str) {
    let origem = "build/firmware_tui.uf2";
    let destino = format!("{}:\\firmware_tui.uf2", letra.to_uppercase());

    if Path::new(origem).exists() {
        println!("Arquivo UF2 localizado. Iniciando transferencia para {}:...", letra);
        if fs::copy(origem, &destino).is_ok() {
            println!("Flash concluido com sucesso!");
        } else {
            eprintln!("Erro: Falha ao copiar. Verifique se o Pico esta no modo BOOTSEL.");
        }
    } else {
        // Esta mensagem confirmara se o Ninja falhou em gerar o arquivo
        eprintln!("Erro Critico: O arquivo '{}' nao foi gerado pelo compilador.", origem);
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
    let cmake_file = "CMakeLists.txt";
    let pico_import_file = "pico_sdk_import.cmake";
    let ninja_path = "C:/Program Files/CMake/bin/ninja.exe";

    // 1. Localizar o SDK (Prioridade para caminhos físicos)
    let mut sdk_path = String::new();
    let locais = ["C:\\pico-sdk", "D:\\pico-sdk"];
    for l in locais {
        if Path::new(l).join("pico_sdk_version.cmake").exists() { 
            sdk_path = l.to_string(); 
            break; 
        }
    }
    
    if sdk_path.is_empty() { sdk_path = std::env::var("PICO_SDK_PATH").unwrap_or_default(); }

    // 2. Gerar CMakeLists.txt com ordem estrita para garantir o UF2
    let cmake_content = format!(
    "cmake_minimum_required(VERSION 3.13)\n\
     set(PICO_NO_PICOTOOL 1)\n\
     include({})\n\
     project(TuiuiuProject C CXX ASM)\n\
     pico_sdk_init()\n\
     add_executable(firmware_tui temp_output.cpp)\n\
     # Mantenha o extra_outputs logo apos o executable
     pico_add_extra_outputs(firmware_tui)\n\
     target_link_libraries(firmware_tui pico_stdlib hardware_flash hardware_watchdog)",
    pico_import_file
);
    fs::write(cmake_file, cmake_content).ok();

    // 3. Limpeza de cache para evitar conflitos de gerador
    let cache_file = format!("{}/CMakeCache.txt", build_dir);
    if Path::new(&cache_file).exists() {
        let _ = fs::remove_file(&cache_file);
    }

    if !Path::new(build_dir).exists() { fs::create_dir(build_dir).ok(); }

    // 4. Configuração e Build
    println!("Configurando CMake via Ninja...");
    let cmake_status = Command::new("cmake")
        .arg("-S").arg(".")
        .arg("-B").arg(build_dir)
        .arg("-G").arg("Ninja")
        .arg(format!("-DCMAKE_MAKE_PROGRAM={}", ninja_path))
        .arg(format!("-DPICO_SDK_PATH={}", sdk_path.replace("\\", "/")))
        .status();

    if let Ok(status) = cmake_status {
        if status.success() {
            println!("Iniciando compilacao...");
            let build_status = Command::new("cmake").arg("--build").arg(build_dir).status();

            if let Ok(b_status) = build_status {
                if b_status.success() {
                    println!("Sucesso: firmware_tui.uf2 gerado.");
                    if let Some(letra) = drive_letra {
                        self.flash_dispositivo(&letra);
                    }
                }
            }
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

    fn transpilador(&mut self, arquivo_tui: &str, _force_hardened: bool, drive: Option<String>) {
    let conteudo = fs::read_to_string(arquivo_tui).unwrap_or_else(|_| {
        eprintln!("Erro: Nao foi possivel ler o arquivo {}", arquivo_tui);
        String::new()
    });
    
    // Identificacao da placa para carregar mapeamentos
    let mut placa_nome = String::new();
    for linha in conteudo.lines() {
        if linha.trim().starts_with("import ") {
            placa_nome = linha.split_whitespace().last().unwrap_or("").replace("\"", "");
        }
    }

    if placa_nome.is_empty() { 
        eprintln!("Erro: Nenhuma placa importada (ex: import \"pico\")");
        return; 
    }

    let _ = self.carregar_mapeamento_fabrica(&placa_nome);
    self.carregar_bibliotecas();
    self.processar_bloco_hardware(&conteudo);

    let mut cpp_output = String::new();
    cpp_output.push_str("#include \"pico/stdlib.h\"\n#include \"hardware/flash.h\"\n#include \"hardware/watchdog.h\"\n\n");
    cpp_output.push_str("int main() {\n\tstdio_init_all();\n");

    for linha in conteudo.lines() {
        let l = linha.trim();
        if l.is_empty() || l.starts_with('.') || l.starts_with('@') || l.starts_with("import") || l.starts_with("//") {
            continue;
        }

        if l.contains("repetir {") {
            cpp_output.push_str("\twhile(true) {\n");
        } else if l == "}" {
            cpp_output.push_str("\t}\n"); 
        } else {
            for (func_tui, snip_c) in &self.biblioteca {
                if l.starts_with(func_tui) {
                    let mut comando_final = snip_c.clone();
                    
                    // Substituicao segura: busca por "(ms)" para nao quebrar "sleep_ms"
                    if let (Some(start), Some(end)) = (l.find('('), l.find(')')) {
                        let arg_valor = &l[start + 1..end];
                        comando_final = comando_final.replace("(ms)", &format!("({})", arg_valor));
                    }
                    
                    // Substituicao de pino baseada no hardware local
                    for (alias, pino_num) in &self.hardware_local {
                        if l.contains(alias) {
                            comando_final = comando_final.replace("pino", &pino_num.to_string());
                        }
                    }

                    let cmd_limpo = comando_final.trim_end_matches(';').to_string() + ";";
                    cpp_output.push_str(&format!("\t\t{}\n", cmd_limpo));
                    break; 
                }
            }
        }
    }

    cpp_output.push_str("\treturn 0;\n}\n");

    if fs::write("temp_output.cpp", &cpp_output).is_ok() {
        println!("Transpilacao concluida: temp_output.cpp gerado.");
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