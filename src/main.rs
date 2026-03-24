use clap::{Parser, Subcommand};
use std::collections::HashMap;
use std::process::Command;
use std::fs;

#[derive(Parser)]
#[command(name = "tui")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[command(short_flag = 'i')]
    Install { nome: String },
    Build { arquivo: String },
}

struct TuiCompiler {
    mapeamento: HashMap<String, u8>,
    // Guarda o nome da funcao e o codigo C++ correspondente
    biblioteca: HashMap<String, String>,
}

impl TuiCompiler {
    fn new() -> Self {
        Self {
            mapeamento: HashMap::new(),
            biblioteca: HashMap::new(),
        }
    }

    fn carregar_mapeamento(&mut self, caminho: &str) {
        if let Ok(conteudo) = fs::read_to_string(caminho) {
            for linha in conteudo.lines() {
                let linha = linha.trim().trim_start_matches('\u{feff}');
                if linha.is_empty() || linha.starts_with("//") { continue; }
                if linha.starts_with('@') {
                    let partes: Vec<&str> = linha.split(':').collect();
                    if partes.len() == 2 {
                        let nome = partes[0].trim().trim_start_matches('@').to_string();
                        let pino = partes[1].trim().parse::<u8>().unwrap_or(0);
                        self.mapeamento.insert(nome, pino);
                    }
                }
            }
        }
    }

    fn carregar_biblioteca(&mut self, caminho: &str) {
        if let Ok(conteudo) = fs::read_to_string(caminho) {
            // Logica para extrair o que esta dentro de NATIVO { ... }
            if let Some(inicio) = conteudo.find("NATIVO {") {
                if let Some(fim) = conteudo.rfind('}') {
                    let codigo_c = &conteudo[inicio + 8..fim].trim();
                    // Por enquanto, associamos a "piscar" de forma fixa para teste
                    self.biblioteca.insert("piscar".to_string(), codigo_c.to_string());
                }
            }
        }
    }

    fn transpilador(&self, arquivo_tui: &str) {
        let conteudo = fs::read_to_string(arquivo_tui).expect("Erro ao ler arquivo .tui");
        let mut cpp_output = String::from("#include \"pico/stdlib.h\"\n\nint main() {\n    stdio_init_all();\n");

        for linha in conteudo.lines() {
            let linha = linha.trim();
            if linha.is_empty() { continue; }

            // Verifica se a linha chama uma funcao da biblioteca (ex: piscar)
            for (func_nome, codigo_c) in &self.biblioteca {
                if linha.contains(func_nome) {
                    let mut snippet = codigo_c.clone();
                    // Substitui o nome do pino pelo numero real do mapeamento
                    for (id_pino, num_pino) in &self.mapeamento {
                        if linha.contains(id_pino) {
                            snippet = snippet.replace("pino", &num_pino.to_string());
                        }
                    }
                    cpp_output.push_str(&format!("    {};\n", snippet));
                }
            }
        }

        cpp_output.push_str("    while(1) { }\n    return 0;\n}");
        fs::write("temp_output.cpp", cpp_output).expect("Erro ao gerar CPP");
        println!("Sucesso: temp_output.cpp gerado com as intervencoes de hardware.");
    }
}

fn compilar_nativo() {
    println!("Compilando binario nativo (.uf2)...");
    
    // 1. O Rust chama o CMake para configurar o projeto do Pico SDK
    let status = Command::new("cmake")
        .arg("-B")
        .arg("build")
        .status()
        .expect("Falha ao executar CMake. Certifique-se que esta no PATH.");

    if status.success() {
        // 2. O Rust chama o Make/Ninja para gerar o arquivo final
        Command::new("cmake")
            .arg("--build")
            .arg("build")
            .status()
            .unwrap();
        
        println!("Sucesso! Arquivo .uf2 gerado na pasta build/");
    }
}

fn main() {
    let cli = Cli::parse();
    let mut compiler = TuiCompiler::new();

    match &cli.command {
        Commands::Install { nome } => {
            let _ = fs::create_dir_all(".tui_libs");
            let _ = fs::create_dir_all(".tui_mapping");
            
            let path_m = format!(".tui_mapping/{}.tui.m", nome);
            let path_l = format!(".tui_libs/{}.tui.l", nome);
            
            fs::write(path_m, "@LED_RGB: 16").unwrap();
            fs::write(path_l, "funcao piscar(pino) {\n    NATIVO {\n        gpio_init(pino);\n        gpio_set_dir(pino, GPIO_OUT);\n        gpio_put(pino, 1);\n    }\n}").unwrap();
            println!("Instalado: {} mapeado para Tuiuiu.", nome);
        }

        Commands::Build { arquivo } => {
            // Carrega todos os mapeamentos (.m)
            if let Ok(entries) = fs::read_dir(".tui_mapping") {
                for entry in entries.flatten() {
                    compiler.carregar_mapeamento(entry.path().to_str().unwrap());
                }
            }
            // Carrega todas as bibliotecas (.l)
            if let Ok(entries) = fs::read_dir(".tui_libs") {
                for entry in entries.flatten() {
                    compiler.carregar_biblioteca(entry.path().to_str().unwrap());
                }
            }
            compiler.transpilador(arquivo);
        }
    }
}