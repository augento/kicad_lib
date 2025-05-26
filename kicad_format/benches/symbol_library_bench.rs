use criterion::{black_box, criterion_group, criterion_main, Criterion};
use kicad_format::symbol_library::SymbolLibraryFile;
use kicad_format::convert::FromSexpr;
use kicad_format::convert::Parser;
use std::fs;
use std::time::Instant;

fn benchmark_symbol_library_parsing(c: &mut Criterion) {
    let test_files = [
        ("Analog.kicad_sym", "tests/symbol_library/Analog.kicad_sym"),
        ("Diode.kicad_sym", "tests/symbol_library/Diode.kicad_sym"),
        ("FPGA_Lattice.kicad_sym", "tests/symbol_library/FPGA_Lattice.kicad_sym"),
        ("LED.kicad_sym", "tests/symbol_library/LED.kicad_sym"),
        ("Oscillator.kicad_sym", "tests/symbol_library/Oscillator.kicad_sym"),
    ];

    for (name, path) in &test_files {
        c.bench_function(&format!("parse_{}", name), |b| {
            let content = fs::read_to_string(path).expect("Failed to read file");
            b.iter(|| {
                let input = black_box(&content);
                let sexpr = kicad_sexpr::from_str(input).expect("Failed to parse sexpr");
                let parser = Parser::new(sexpr.as_list().unwrap().clone());
                let _result = SymbolLibraryFile::from_sexpr(parser).expect("Failed to parse symbol library");
            });
        });
    }
}

fn profile_parsing() {
    println!("\nDetailed parsing performance analysis:");
    println!("{}", "=".repeat(60));
    
    let test_files = [
        ("Analog.kicad_sym", "tests/symbol_library/Analog.kicad_sym"),
        ("Diode.kicad_sym", "tests/symbol_library/Diode.kicad_sym"),
        ("FPGA_Lattice.kicad_sym", "tests/symbol_library/FPGA_Lattice.kicad_sym"),
        ("LED.kicad_sym", "tests/symbol_library/LED.kicad_sym"),
        ("Oscillator.kicad_sym", "tests/symbol_library/Oscillator.kicad_sym"),
    ];

    for (name, path) in &test_files {
        let content = fs::read_to_string(path).expect("Failed to read file");
        let file_size_kb = content.len() as f64 / 1024.0;
        
        // Time sexpr parsing
        let start = Instant::now();
        let sexpr = kicad_sexpr::from_str(&content).expect("Failed to parse sexpr");
        let sexpr_time = start.elapsed();
        
        // Time symbol library parsing
        let start = Instant::now();
        let parser = Parser::new(sexpr.as_list().unwrap().clone());
        let result = SymbolLibraryFile::from_sexpr(parser).expect("Failed to parse symbol library");
        let parse_time = start.elapsed();
        
        let total_time = sexpr_time + parse_time;
        let throughput_mb_s = (content.len() as f64 / 1024.0 / 1024.0) / total_time.as_secs_f64();
        
        println!("\n{}", name);
        println!("  File size: {:.2} KB", file_size_kb);
        println!("  S-expression parsing: {:?}", sexpr_time);
        println!("  Symbol library parsing: {:?}", parse_time);
        println!("  Total time: {:?}", total_time);
        println!("  Throughput: {:.2} MB/s", throughput_mb_s);
        println!("  Symbols parsed: {}", result.symbols.len());
        if !result.symbols.is_empty() {
            println!("  Time per symbol: {:.2} µs", 
                     parse_time.as_micros() as f64 / result.symbols.len() as f64);
        }
    }
}

criterion_group!(benches, benchmark_symbol_library_parsing);
criterion_main!(benches);

#[test]
fn run_profile() {
    profile_parsing();
}