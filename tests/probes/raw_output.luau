-- Run without the wrapper (`studio-run raw`). A short print, a pause so the
-- output file and the live log can be compared mid-run, then a long line.
-- Strings are split so Studio's echo of this source doesn't match them.
print("PROBE" .. "-early")
task.wait(10)
print("PROBE" .. "-LONG:" .. string.rep("x", 5000))
print("PROBE" .. "-late")
