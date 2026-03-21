Write a function called `safe-sqrt` that takes an integer and returns the integer square root (floor of square root).

Requirements:
- The input must be >= 0
- If the input is negative, return an error value (not a crash)
- If the input is non-negative, return Ok with the floor square root
- The function must ensure the result >= 0 for valid inputs

Test case: safe-sqrt(16) should return Ok(4)
Test case: safe-sqrt(10) should return Ok(3)
Test case: safe-sqrt(-1) should return an error

Print the result of calling the function with argument 25.
