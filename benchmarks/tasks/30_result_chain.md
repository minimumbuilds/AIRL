Write a function `parse-and-double` that parses a string as an integer, doubles it, and returns Ok(result) or Err("not a number").

Requirements:
- Return Ok with double the integer value if input is a valid integer
- Return Err with message "not a number" if input cannot be parsed
- Print the result

---TESTS---
parse-and-double("21") => Ok(42)
parse-and-double("abc") => Err(not a number)
parse-and-double("0") => Ok(0)
