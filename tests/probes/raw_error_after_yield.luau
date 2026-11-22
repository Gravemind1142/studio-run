-- Run without the wrapper (`studio-run raw`). An uncaught error after the
-- script has yielded once.
task.wait(1)
print("PROBE" .. "-before")
error("PROBE" .. "-uncaught")
