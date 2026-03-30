Write a function called `apply-n-times` that takes a function, a value, and a count N, and applies the function to the value N times.

Requirements:
- apply-n-times(f, x, 0) returns x
- apply-n-times(f, x, 3) returns f(f(f(x)))
- The function parameter should have type annotation `fn` in the :sig
- N must be non-negative (enforce in :requires)

Test case: apply-n-times(double, 1, 3) where double(x) = x*2 should return 8
Test case: apply-n-times(increment, 0, 5) where increment(x) = x+1 should return 5

Define a function `double` that doubles its argument. Print the result of calling apply-n-times with double, 3, and 4.
