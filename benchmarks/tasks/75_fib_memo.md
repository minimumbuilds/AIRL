Write a function called `fib-fast` that computes the N-th Fibonacci number efficiently using an accumulator pattern (tail-recursive style).

Requirements:
- fib-fast(0) = 0, fib-fast(1) = 1
- Use two accumulators (a, b) passed through recursion instead of naive double recursion
- Must handle N up to 30 without excessive computation
- Input must be non-negative

Test case: fib-fast(0) should return 0
Test case: fib-fast(10) should return 55
Test case: fib-fast(20) should return 6765

Print the result of calling the function with argument 25.
