import { Button, Paper, PasswordInput, Text, Title } from "@mantine/core";
import { useState, useEffect } from "react";
import { tauriAPI } from "../../lib/tauri";

export function ApiKeySettings() {
	const [apiKey, setApiKey] = useState("");
	const [saved, setSaved] = useState(false);

	useEffect(() => {
		tauriAPI.getSettings().then(settings => {
			setApiKey(settings.openai_api_key ?? "");
		});
	}, []);

	const handleSave = async () => {
		await tauriAPI.updateOpenaiApiKey(apiKey || null);
		setSaved(true);
		setTimeout(() => setSaved(false), 2000);
	};

	return (
		<Paper p="md" radius="md" className="animate-in" mb="md">
			<Title order={4} mb="xs">API Key</Title>
			<Text c="dimmed" size="sm" mb="md">
				Enter your OpenAI API key for speech-to-text and optional LLM formatting.
			</Text>
			<PasswordInput
				label="OpenAI API Key"
				placeholder="sk-..."
				value={apiKey}
				onChange={(e) => setApiKey(e.currentTarget.value)}
				mb="sm"
			/>
			<Button onClick={handleSave} size="sm" variant="light">
				{saved ? "Saved!" : "Save"}
			</Button>
		</Paper>
	);
}
