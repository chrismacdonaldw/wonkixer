use assert_no_alloc::{AllocDisabler, assert_no_alloc};
use wonkixer_dsp::{Chain, ParamValues, StageId};

#[global_allocator]
static ALLOCATOR: AllocDisabler = AllocDisabler;

const BLOCK: usize = 480;

fn fill(block: &mut [f32], seed: &mut u32, phase: &mut f32, quiet: bool) {
    for sample in block.iter_mut() {
        *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let noise = ((*seed >> 9) as f32 / (1u32 << 23) as f32) - 1.0;
        *phase += 0.03;
        let tone = (*phase).sin();
        *sample = if quiet {
            noise * 0.0005
        } else {
            tone * 0.3 + noise * 0.02
        };
    }
}

#[test]
fn processing_never_allocates_on_a_fresh_thread_after_warm_up() {
    let mut chain = Chain::new(48_000);
    let mut values = ParamValues::default();
    values.set_bypassed(StageId::NoiseSuppressor, false);
    chain.apply(&values);
    std::thread::spawn(move || {
        chain.warm_up();
        let mut block = vec![0.0f32; BLOCK];
        let mut seed = 7u32;
        let mut phase = 0.0f32;
        let kept = assert_no_alloc(|| {
            for index in 0..600 {
                let quiet = (index / 40) % 3 == 2;
                fill(&mut block, &mut seed, &mut phase, quiet);
                chain.process_block(&mut block);
            }
            (chain, block)
        });
        drop(kept);
    })
    .join()
    .expect("join");
}
