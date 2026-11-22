-- Run without the wrapper (`studio-run raw`). An error in a spawned thread,
-- a delayed print after the chunk returns, then an uncaught top-level error.
-- Strings are split so Studio's echo of this source doesn't match them.
task.spawn(function()
	error("PROBE" .. "-spawned-error")
end)
task.delay(1, function()
	print("PROBE" .. "-delayed")
end)
print("PROBE" .. "-before")
error("PROBE" .. "-uncaught")
