Write a function called `build-phonebook` that takes a list of [name, number] pairs and returns a map from names to numbers.

Requirements:
- Each pair is a two-element list where the first element is the name and the second is the number
- If a name appears multiple times, the last occurrence wins
- Return a map (not a list of pairs)

Test case: build-phonebook([["Alice", "555-1234"], ["Bob", "555-5678"]]) should return a map with Alice->555-1234, Bob->555-5678

Print the result of calling the function with argument [["Alice", "111"], ["Bob", "222"], ["Alice", "333"]].
