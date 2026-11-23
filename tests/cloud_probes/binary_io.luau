-- Reads the binary input and returns it reversed as the binary output.
local args = { ... }
local input = args[1]
local data = input and input.BinaryInput
local n = if data then buffer.len(data) else -1
local out = buffer.create(math.max(n, 0))
for i = 0, n - 1 do
	buffer.writeu8(out, i, buffer.readu8(data, n - 1 - i))
end
local keys = {}
if type(input) == "table" then
	for k in input do
		table.insert(keys, tostring(k))
	end
end
return {
	BinaryOutput = out,
	ReturnValues = { n, typeof(input), keys },
}
