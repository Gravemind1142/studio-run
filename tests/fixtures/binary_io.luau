-- Prints the inputs' names and total size, and sets the binary output to the
-- inputs in name order, each reversed.
local ctx = ...
local names = {}
for name in ctx.inputs do
	table.insert(names, name)
end
table.sort(names)
local total = 0
for _, name in names do
	total += buffer.len(ctx.inputs[name])
end
print(table.concat(names, ","), total)

local out = buffer.create(total)
local pos = 0
for _, name in names do
	local data = ctx.inputs[name]
	local len = buffer.len(data)
	for i = 0, len - 1 do
		buffer.writeu8(out, pos + i, buffer.readu8(data, len - 1 - i))
	end
	pos += len
end
ctx.setBinaryOutput(out)
