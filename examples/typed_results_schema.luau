--!strict
-- Typed-result review schema. Keeping the JSON Schema in its own module
-- lets scripts reuse it across sessions and review the shape in isolation.
-- Plain Luau tables: no builder DSL, `{}`-free arrays, everything ponos's
-- `result` option accepts.
export type ReviewSchema = { [string]: any }

local review: ReviewSchema = {
	type = "object",
	properties = {
		verdict = { type = "string", enum = { "approve", "block" } },
		score = { type = "integer", minimum = 0, maximum = 10 },
	},
	required = { "verdict", "score" },
}

return { review = review }
