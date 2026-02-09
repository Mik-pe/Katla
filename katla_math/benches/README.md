# katla_math Benchmarks

This directory contains performance benchmarks for the katla_math library using Criterion.rs.

## Purpose

These benchmarks establish performance baselines for all critical math operations. Before making any optimization changes:

1. **Run the benchmarks** to establish a baseline
2. **Make your changes**
3. **Run the benchmarks again** to verify improvements
4. **Compare results** using Criterion's reports

This ensures we optimize based on actual data, not assumptions.

## Running Benchmarks

### Run All Benchmarks

```bash
cargo bench
```

### Run a Specific Benchmark

```bash
cargo bench --bench vec3_bench
cargo bench --bench mat4_bench
cargo bench --bench quat_bench
cargo bench --bench transform_bench
```

### Save Baseline for Comparison

```bash
# Run benchmarks and save as baseline
cargo bench -- --save-baseline main
```

### Compare Against Baseline

```bash
# Run new benchmarks and compare against saved baseline
cargo bench -- --baseline main
```

### Generate HTML Report

After running benchmarks, Criterion generates an HTML report at:

```
target/criterion/reports/index.html
```

Open this file in a browser to see detailed performance visualizations.

## Benchmark Categories

### vec3_bench.rs
- **Arithmetic**: add, sub, mul, div (scalar and vector), negation
- **Operations**: dot, cross, length, normalize, lerp
- **Accessors**: x(), y(), z(), indexing
- **Assignment**: +=, -=, *=, /=
- **Comparison**: equality checks
- **Creation**: constructors, constants

### mat4_bench.rs
- **Creation**: identity, translation, rotation, orthographic, projection, look-at
- **Arithmetic**: matrix multiplication
- **Linear Algebra**: determinant, inverse, row extraction
- **Indexing**: column and element access
- **Transformations**: TRS (Translation-Rotation-Scale) composition
- **Inverse Scenarios**: identity, translation, rotation, complex transforms

### quat_bench.rs
- **Creation**: axis-angle, rotation between vectors, yaw-pitch
- **Arithmetic**: quaternion multiplication
- **Operations**: dot, inverse, normalize, is_normalized, length_squared
- **Rotation**: rotate vectors, quaternion × vector
- **Interpolation**: SLERP at different t values (0.0, 0.5, 1.0)
- **Matrix Conversion**: quaternion to Mat4
- **Indexing**: component access and xyzw() tuple

### transform_bench.rs
- **Creation**: from position, rotation, scale
- **Arithmetic**: transform composition, transform × vector
- **Matrix Generation**: make_mat4() for various transform types
- **Hierarchy**: simulating scene graph hierarchies
- **Composition**: combining transforms

## Interpreting Results

### What to Look For

1. **Regression**: Performance got worse (red bar in HTML report)
2. **Improvement**: Performance got better (green bar in HTML report)
3. **Noise**: Normal variation (gray bar in HTML report)

### Benchmark Output

```
vec3_arithmetic/add          time:   [2.1234 ns 2.1456 ns 2.1678 ns]
                        change: [-2.3% +0.5% +3.1%] (p = 0.45 > 0.05)
                        No change in performance detected.
```

- **time**: [min median max] nanoseconds per iteration
- **change**: percentage change from baseline
- **p-value**: statistical significance (p < 0.05 means significant change)

### Threshold for Significance

Criterion uses statistical testing to determine if changes are significant. Small variations (< 5%) are often noise. Look for:

- **Consistent improvements**: > 5% faster
- **Meaningful regressions**: > 5% slower

## Benchmarking Best Practices

### Before Optimizing

1. Run benchmarks to find actual bottlenecks
2. Profile the code to understand why it's slow
3. Consider algorithmic improvements before micro-optimizations

### When Optimizing

1. **Change ONE thing at a time** - makes it clear what caused the change
2. **Re-run benchmarks** after each change
3. **Check correctness** - ensure optimization didn't break functionality
4. **Document the change** - note why it's faster in code comments

### Common Pitfalls

- **Premature optimization**: Optimizing code that's not a bottleneck
- **Compiler interference**: Debug builds are not representative (use `--release`)
- **Thermal throttling**: Let CPU cool between long benchmark runs
- **Background processes**: Close other apps for consistent results

## Continuous Integration

To integrate benchmarks into CI:

```yaml
# .github/workflows/benchmark.yml
- name: Run benchmarks
  run: cargo bench -- --save-baseline ci

- name: Compare with main
  run: cargo bench -- --baseline main
```

Note: CI environments are noisy, so use higher thresholds for significance.

## Adding New Benchmarks

When adding new types or methods to katla_math:

1. Create a new benchmark file or add to existing ones
2. Follow existing naming patterns: `bench_<type>_<operation>`
3. Use `black_box()` to prevent compiler optimization
4. Test representative use cases
5. Group related operations with `c.benchmark_group()`

Example:

```rust
fn bench_my_new_operation(c: &mut Criterion) {
    let mut group = c.benchmark_group("my_type_operations");

    let input = setup_test_data();

    group.bench_function("my_operation", |b| {
        b.iter(|| black_box(my_operation(input)));
    });

    group.finish();
}
```

## Current Baseline

As of the initial setup, these benchmarks establish the baseline for:

- Scalar implementation of all math operations
- No SIMD acceleration
- Current Rust compiler optimizations

Future work will:

1. Add SIMD implementations
2. Compare SIMD vs scalar performance
3. Optimize based on actual profiling data
4. Document which operations benefit most from SIMD

## Resources

- [Criterion.rs User Guide](https://bheisler.github.io/criterion.rs/book/index.html)
- [Rust Performance Book](https://nnethercote.github.io/perf-book/benchmarks.html)
- [IEEE 754 Floating Point](https://en.wikipedia.org/wiki/IEEE_754) - for understanding precision tradeoffs
