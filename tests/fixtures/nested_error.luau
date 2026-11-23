local function inner()
	error("deep boom")
end
local function outer()
	inner()
end
outer()
