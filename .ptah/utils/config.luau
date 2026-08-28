--!strict

export type Config = {
	defaultModel: string,
	judgeModel: string,
	changeName: string,
}

return {
	defaultModel = "zai-custom/glm-5.3",
	judgeModel = "zai-custom/glm-5.3-flash",
	changeName = "pi-acp-mcp-wiring",
} :: Config
