--!strict

export type Config = {
	defaultModel: string,
	judgeModel: string,
	changeName: string,
}

return {
	defaultModel = "zai/glm-5.3",
	judgeModel = "qwencloud/deepseek-v4-flash-0731",
	changeName = "hexagonal-workspace-split",
} :: Config
