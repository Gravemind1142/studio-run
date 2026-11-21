-- Run jest-roblox tests with studio-run:
--
--   studio-run run examples/jest.luau --place tests.rbxl
--
-- Assumes Jest and your tests were built into the place (e.g. with Rojo and
-- Wally). Adjust the two paths below to match your project.
--
-- Not yet tested against a real jest-roblox project.

local ReplicatedStorage = game:GetService("ReplicatedStorage")

local Jest = require(ReplicatedStorage.DevPackages.Jest)
local root = ReplicatedStorage.Tests

local status, result = Jest.runCLI(root, { ci = true, verbose = false }, { root }):awaitStatus()

if status == "Rejected" then
	error(tostring(result))
end

-- Jest's own entry scripts call ProcessService:ExitAsync here, which doesn't
-- exist under Studio's CLI. Returning the result sets studio-run's exit code.
return result.results.success
