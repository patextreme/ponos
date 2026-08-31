--!strict

export type Config = {
	defaultModel: string,
	judgeModel: string,
	changeName: string,
}

return {
	defaultModel = "zai/glm-5.3",
	judgeModel = "zai/glm-5.3-flash",
	changeName = "harden-signal-teardown",
} :: Config
