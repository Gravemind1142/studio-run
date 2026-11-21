-- Minimal smoke test for `--task RunScript`.
print("hello from studio-run")
print("IsStudio:", game:GetService("RunService"):IsStudio())
print("PlaceId:", game.PlaceId, "Workspace children:", #workspace:GetChildren())
warn("this is a warning")
