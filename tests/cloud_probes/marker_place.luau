-- Reports which place version the task loaded and whether it's tests/fixtures/marker.rbxlx.
return {
	placeVersion = game.PlaceVersion,
	marker = workspace:FindFirstChild("MarkerPart") ~= nil,
	baseplate = workspace:FindFirstChild("Baseplate") ~= nil,
}
