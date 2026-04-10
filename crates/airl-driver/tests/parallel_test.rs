/// Parallel invocation stress tests — verifies run_source is safe under
/// concurrent execution (issue-058: eliminate RUST_TEST_THREADS=1 workaround).

#[test]
fn parallel_arithmetic() {
    // 4 threads, each calling run_source 50 times with arithmetic
    let handles: Vec<_> = (0..4).map(|i| {
        std::thread::spawn(move || {
            let src = format!("(+ {} {})", i * 10, i * 10);
            for _ in 0..50 {
                let result = airl_driver::pipeline::run_source(&src);
                assert!(result.is_ok(), "arithmetic failed: {:?}", result);
            }
        })
    }).collect();
    for h in handles { h.join().expect("thread panicked"); }
}

#[test]
fn parallel_stdlib_calls() {
    // 4 threads exercising stdlib functions concurrently
    let sources = [
        "(map (fn [x] (* x x)) [1 2 3 4 5])",
        "(filter (fn [x] (> x 2)) [1 2 3 4 5])",
        "(fold (fn [acc x] (+ acc x)) 0 [1 2 3 4 5])",
        "(length [1 2 3 4 5])",
    ];
    let handles: Vec<_> = sources.iter().enumerate().map(|(i, src)| {
        let src = src.to_string();
        std::thread::spawn(move || {
            for _ in 0..25 {
                let result = airl_driver::pipeline::run_source(&src);
                assert!(result.is_ok(), "stdlib call {} failed: {:?}", i, result);
            }
        })
    }).collect();
    for h in handles { h.join().expect("thread panicked"); }
}
