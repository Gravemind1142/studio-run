-- Throws from a nested function on line 4, called from line 7.
print("before the error")
local function inner()
	error("probe runtime error")
end
local function outer()
	inner()
end
outer()
