-- Prints over time, to check whether output can be read while Studio runs.
for i = 1, 4 do
	print("tick " .. i)
	task.wait(2)
end
