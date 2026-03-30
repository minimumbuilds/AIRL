Write two functions: `parse-positive` that parses a string as an integer and rejects non-positive values, and `double-if-small` that doubles a number only if it's less than 100.

Requirements:
- parse-positive returns (Ok n) if the string is a valid positive integer, (Err message) otherwise
- double-if-small returns (Ok (* n 2)) if n < 100, (Err "too large") otherwise
- Chain them: parse the string, then if Ok, pass to double-if-small
- Use nested match to propagate errors through the chain

Call parse-positive with "42", then pass the result to double-if-small using match. Print the final result.
