Write a function called `bounded-access` that safely accesses a list at a given index.

Requirements:
- If the index is within bounds (0 to length-1), return Ok with the element
- If the index is out of bounds, return Err with "out of bounds"
- The function must not crash on any index value

Test case: bounded-access([10, 20, 30], 1) should return Ok(20)
Test case: bounded-access([10, 20, 30], 5) should return Err("out of bounds")

Print the result of calling the function with arguments [10, 20, 30, 40, 50] and 2.
