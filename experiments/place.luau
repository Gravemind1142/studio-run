-- Expects experiments/marker.rbxlx to be loaded; then modifies the place
-- to check whether quitting prompts to save.
print("MarkerPart found:", workspace:FindFirstChild("MarkerPart") ~= nil)
workspace.MarkerPart:Destroy()
Instance.new("Folder", workspace).Name = "AddedByScript"
print("modified place")
