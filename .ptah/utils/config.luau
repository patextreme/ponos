--!strict

export type Config = {
	defaultModel: string,
	judgeModel: string,
	changeName: string,
}

return {
	defaultModel = "zai-custom/glm-5.3-flash",
	judgeModel = "qwencloud/deepseek-v4-flash-0731",
	changeName = "rename-to-ptah",
} :: Config
