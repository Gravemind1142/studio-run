-- Prints, waits, prints again: used to read logs and try cancelling mid-run.
print("PROBE" .. "-early")
warn("PROBE" .. "-early-warn")
task.wait(40)
print("PROBE" .. "-late")
return "finished"
