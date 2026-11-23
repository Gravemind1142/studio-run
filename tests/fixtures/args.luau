-- Prints the parts of the script context that are the same on both hosts.
-- (ctx.host and ctx.place say where the script ran, so they differ.)
local ctx = ...
local keys = {}
for key in ctx.args do
	table.insert(keys, key)
end
table.sort(keys)
for _, key in keys do
	print(key, "=", ctx.args[key])
end
print("version", ctx.version)
