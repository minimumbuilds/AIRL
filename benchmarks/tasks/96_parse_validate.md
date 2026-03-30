Write a function called `parse-color` that takes a string and returns a Result with a structured color representation.

Requirements:
- Accept formats: "red", "green", "blue" (named colors), or "rgb(R,G,B)" format
- Named colors return (Ok [255 0 0]), (Ok [0 255 0]), (Ok [0 0 255]) respectively
- For "rgb(R,G,B)", parse the three numbers and validate they are 0-255
- Return (Err "unknown color") for unrecognized names
- Return (Err "invalid rgb") for malformed rgb strings

Test case: parse-color("red") should return (Ok [255 0 0])
Test case: parse-color("purple") should return (Err "unknown color")

Print the result of calling the function with argument "blue".
