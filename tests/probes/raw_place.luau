-- Run without the wrapper (`studio-run raw`) against tests/fixtures/marker.rbxlx.
-- Changes the place, to check it isn't saved when Studio quits.
print("PROBE-marker-found:" .. tostring(workspace:FindFirstChild("MarkerPart") ~= nil))
workspace.MarkerPart:Destroy()
Instance.new("Folder", workspace).Name = "AddedByScript"
print("PROBE" .. "-changed")
