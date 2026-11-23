-- Structured log messages: LogService and Logger calls with a context table.
local LogService = game:GetService("LogService")

LogService:Info("small", { n = 1, s = "x", b = true })
-- A context too long for one event: sent in pieces.
LogService:Warn("big", { blob = string.rep("é", 400), other = "end" })
-- A message too long for one event, with a context after it.
LogService:Info(string.rep("m", 1500) .. "END", { k = "long message" })
LogService:GetLogger("Tests"):Error("from logger", { who = "logger" })
LogService:Info("no context")
LogService:Info("empty context", {})
print("plain")
