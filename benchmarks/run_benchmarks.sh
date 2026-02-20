#!/bin/bash
# Benchmark runner for the C compiler
# Compiles benchmarks with both our compiler and GCC, measures execution time

benchmarks=(
    "fib"
    "array_sum"
    "matmul"
    "bitwise"
    "struct_bench"
)

results_file="benchmarks/results_linux.md"

echo "=== C Compiler Benchmark Suite ==="
echo ""

# Build our compiler
echo "Building compiler..."
cargo build --release > /dev/null 2>&1
if [ $? -ne 0 ]; then
    echo "Failed to build compiler!"
    exit 1
fi
our_compiler="target/release/driver"

# Function to measure execution time in milliseconds (floating point)
measure_time() {
    local executable=$1
    local start=$(date +%s%N)
    $executable > /dev/null 2>&1
    local end=$(date +%s%N)
    # Convert nanoseconds to milliseconds with decimal precision
    echo "$start $end" | awk '{printf "%.3f", ($2 - $1) / 1000000.0}'
}

# Function to calculate trimmed mean (floating point)
trimmed_mean() {
    local values=("$@")
    local count=${#values[@]}
    
    # Sort the values using awk for floating point
    IFS=$'\n' sorted=($(printf '%s\n' "${values[@]}" | sort -n))
    unset IFS
    
    # Remove top and bottom 20%
    local remove_count=$(($count / 5))
    local start_idx=$remove_count
    local end_idx=$(($count - $remove_count - 1))
    
    # Calculate mean of middle values using awk
    local sum=0
    local kept_values=""
    for ((i=$start_idx; i<=$end_idx; i++)); do
        if [ -z "$kept_values" ]; then
            kept_values="${sorted[$i]}"
        else
            kept_values="$kept_values ${sorted[$i]}"
        fi
    done
    
    echo "$kept_values" | awk '{sum=0; for(i=1; i<=NF; i++) sum+=$i; printf "%.2f", sum/NF}'
}

# Initialize results
cat > $results_file << EOF
# Benchmark Results (Linux)

Generated: $(date "+%Y-%m-%d %H:%M:%S")

| Benchmark | Our Compiler (ms) | GCC -O0 (ms) | GCC -O2 (ms) | Speedup vs GCC-O0 | Speedup vs GCC-O2 |
|-----------|-------------------|--------------|--------------|-------------------|-------------------|
EOF

for bench in "${benchmarks[@]}"; do
    echo "Running benchmark: $bench"
    
    source_file="benchmarks/$bench.c"
    
    # Compile with our compiler
    echo "  Compiling with our compiler..."
    $our_compiler $source_file > /dev/null 2>&1
    if [ $? -ne 0 ]; then
        echo "  Failed to compile $bench with our compiler!"
        continue
    fi
    our_exe="./$bench"
    
    # Compile with GCC -O0
    echo "  Compiling with GCC -O0..."
    gcc -O0 $source_file -o "benchmarks/${bench}_gcc_o0" > /dev/null 2>&1
    
    # Compile with GCC -O2
    echo "  Compiling with GCC -O2..."
    gcc -O2 $source_file -o "benchmarks/${bench}_gcc_o2" > /dev/null 2>&1
    
    # Run benchmarks with warmup and outlier filtering
    echo "  Running benchmarks..."
    
    warmup_runs=10
    measure_runs=50
    
    our_times=()
    gcc_o0_times=()
    gcc_o2_times=()
    
    # Warmup runs (don't measure)
    for ((i=0; i<$warmup_runs; i++)); do
        $our_exe > /dev/null 2>&1
        benchmarks/${bench}_gcc_o0 > /dev/null 2>&1
        benchmarks/${bench}_gcc_o2 > /dev/null 2>&1
    done
    
    # Measurement runs
    for ((i=0; i<$measure_runs; i++)); do
        our_times+=($(measure_time $our_exe))
        gcc_o0_times+=($(measure_time benchmarks/${bench}_gcc_o0))
        gcc_o2_times+=($(measure_time benchmarks/${bench}_gcc_o2))
    done
    
    # Calculate trimmed means
    our_avg=$(trimmed_mean "${our_times[@]}")
    gcc_o0_avg=$(trimmed_mean "${gcc_o0_times[@]}")
    gcc_o2_avg=$(trimmed_mean "${gcc_o2_times[@]}")
    
    # Calculate speedups (using awk for floating point, handle division by zero)
    speedup_o0=$(awk -v gcc="$gcc_o0_avg" -v ours="$our_avg" 'BEGIN {if (ours > 0) printf "%.2f", gcc / ours; else print "N/A"}')
    speedup_o2=$(awk -v gcc="$gcc_o2_avg" -v ours="$our_avg" 'BEGIN {if (ours > 0) printf "%.2f", gcc / ours; else print "N/A"}')
    
    echo "    Our compiler: $our_avg ms"
    echo "    GCC -O0:      $gcc_o0_avg ms (${speedup_o0}x)"
    echo "    GCC -O2:      $gcc_o2_avg ms (${speedup_o2}x)"
    
    # Append to results
    echo "| $bench | $our_avg | $gcc_o0_avg | $gcc_o2_avg | ${speedup_o0}x | ${speedup_o2}x |" >> $results_file
    
    # Cleanup
    rm -f $our_exe
    rm -f "benchmarks/${bench}_gcc_o0"
    rm -f "benchmarks/${bench}_gcc_o2"
done

# Add notes
cat >> $results_file << EOF

## Notes
- Measurement methodology: 10 warmup runs + 50 measured runs per benchmark
- Times are trimmed mean (remove top/bottom 20%, average middle 60%) to filter outliers
- Speedup > 1.0 means our compiler is faster
- GCC -O0 is no optimization, GCC -O2 is standard optimizations
EOF

echo ""
echo "Results saved to $results_file"
