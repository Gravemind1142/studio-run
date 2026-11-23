-- Prints the script context as JSON, for the tests of --arg and ctx.
local ctx = ...
print(game:GetService("HttpService"):JSONEncode({
	host = ctx.host,
	version = ctx.version,
	args = ctx.args,
	place = ctx.place,
}))
