Write a function called `parse-int` that takes a string and attempts to parse it as an integer.

Requirements:
- If the string represents a valid integer, return Ok with the integer value
- If the string is not a valid integer, return Err with a reason string
- Handle optional leading minus sign for negative numbers

Test case: parse-int("42") should return Ok(42)
Test case: parse-int("-7") should return Ok(-7)
Test case: parse-int("abc") should return Err("not a number")

Print the result of calling the function with argument "123".
