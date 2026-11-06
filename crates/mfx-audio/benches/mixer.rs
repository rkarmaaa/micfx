//! ns per frame del mixer con 1, 8 e 32 voci (SPEC §15.3).

use std::hint::black_box;
use std::sync::Arc;
use std::sync::atomic::AtomicU32;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use mfx_audio::{Mixer, MixerCmd, SoundRef};

fn bench_render(c: &mut Criterion) {
    let mut group = c.benchmark_group("mixer_render_480");
    let samples: Vec<i16> = (0..48_000 * 2 * 60).map(|i| ((i * 31) % 4000) as i16 - 2000).collect();
    let src: Arc<(Vec<i16>, u16)> = Arc::new((samples, 2));
    for voices in [1u64, 8, 32] {
        group.throughput(Throughput::Elements(480));
        group.bench_with_input(BenchmarkId::from_parameter(voices), &voices, |b, &voices| {
            let mut m = Mixer::new(Arc::new(AtomicU32::new(0.8f32.to_bits())), -1.0);
            let mut sink = |_: u64, _: SoundRef| Ok(());
            for id in 0..voices {
                m.apply(MixerCmd::Play { play_id: id, sound: SoundRef::new(src.clone()), gain: 0.5 }, &mut sink);
            }
            let mut out = vec![0.0f32; 960];
            b.iter(|| {
                m.render(black_box(&mut out), &mut sink);
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_render);
criterion_main!(benches);
