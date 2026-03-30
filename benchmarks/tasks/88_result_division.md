Write a function called `safe-divide-result` that divides two integers and returns a Result type.

Requirements:
- Return (Ok quotient) when the divisor is non-zero
- Return (Err "division by zero") when the divisor is zero
- Use integer division
- The caller should be able to match on the Result

Test case: safe-divide-result(10, 3) should return (Ok 3)
Test case: safe-divide-result(10, 0) should return (Err "division by zero")

Call the function with arguments 15 and 4, then match on the result: print "Result: N" for Ok or "Error: msg" for Err.
