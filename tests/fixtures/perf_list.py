"""Performance test fixtures for AIRL v0.3.0 — Python comparison baseline."""

import time

# 1. Sum 10K integers
start = time.time()
result = sum(range(10000))
print(f"Sum 10K: {result} in {(time.time()-start)*1000:.1f}ms")
assert result == 49995000

# 2. Filter evens from 10K
start = time.time()
result = len([x for x in range(10000) if x % 2 == 0])
print(f"Filter evens 10K: {result} in {(time.time()-start)*1000:.1f}ms")
assert result == 5000

# 3. Map squares over 1K
start = time.time()
result = [x*x for x in range(1000)]
print(f"Map squares 1K: last = {result[-1]} in {(time.time()-start)*1000:.1f}ms")
assert result[-1] == 998001

# 4. Sort 100 integers in reverse
start = time.time()
result = sorted(list(reversed(range(100))))
print(f"Sort 100 reversed: last = {result[-1]} in {(time.time()-start)*1000:.1f}ms")
assert result == list(range(100))

# 5. Chained: sum of squares of even numbers 0..99
start = time.time()
result = sum(x*x for x in range(100) if x % 2 == 0)
print(f"Chained sum-sq-evens: {result} in {(time.time()-start)*1000:.1f}ms")
assert result == 161700
