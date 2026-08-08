use criterion::{Criterion, criterion_group, criterion_main};
use vietnamese_core::{EngineConfig, InputEngine, KeyEvent};

fn process_key_benchmark(criterion: &mut Criterion) {
    criterion.bench_function("process_key_telex_tieengs", |bencher| {
        bencher.iter(|| {
            let mut engine = InputEngine::new(EngineConfig::default());
            for character in "tieengs".chars() {
                std::hint::black_box(engine.process_key(KeyEvent::character(character)));
            }
        });
    });
}

criterion_group!(benches, process_key_benchmark);
criterion_main!(benches);
