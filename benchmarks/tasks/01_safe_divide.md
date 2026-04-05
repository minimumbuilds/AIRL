Write a function called `safe-divide` that takes two integers `a` and `b` and returns their quotient.

Requirements:
- If `b` is zero, return an error/failure value (not a crash)
- If `b` is non-zero, return the integer division result
- The function must validate that `b` is not zero before dividing

Print the result of calling the function with arguments 10 and 3.

---TESTS---
safe-divide(10, 3) => 3
safe-divide(10, 0) => error
safe-divide(0, 5) => 0
safe-divide(-6, 2) => -3
