import {
	ActionIcon,
	Button,
	Group,
	Paper,
	PasswordInput,
	Radio,
	Select,
	Text,
	TextInput,
	Textarea,
	Title,
	Stack,
	Divider,
} from "@mantine/core";
import { useState, useEffect, useCallback } from "react";
import { tauriAPI, type SttProvider } from "../../lib/tauri";

const DEFAULT_PROVIDER: SttProvider = {
	name: "OpenAI",
	base_url: "https://api.openai.com/v1/audio/transcriptions",
	model: "whisper-1",
	api_key: "",
	request_format: "multipart",
	extra_body: null,
};

export function ApiKeySettings() {
	const [providers, setProviders] = useState<SttProvider[]>([DEFAULT_PROVIDER]);
	const [activeIndex, setActiveIndex] = useState(0);
	const [sttPrompt, setSttPrompt] = useState("");
	const [pastePrefix, setPastePrefix] = useState("");
	const [openaiApiKey, setOpenaiApiKey] = useState("");
	const [openaiSaved, setOpenaiSaved] = useState(false);

	useEffect(() => {
		tauriAPI.getSettings().then(settings => {
			const p = settings.stt_providers.length > 0 ? settings.stt_providers : [DEFAULT_PROVIDER];
			setProviders(p);
			setActiveIndex(settings.active_stt_provider_index);
			setSttPrompt(settings.stt_prompt ?? "");
			setPastePrefix(settings.paste_prefix ?? "");
			setOpenaiApiKey(settings.openai_api_key ?? "");
		});
	}, []);

	const saveProviders = useCallback(async (newProviders: SttProvider[]) => {
		setProviders(newProviders);
		await tauriAPI.updateSttProviders(newProviders);
	}, []);

	const saveActiveIndex = useCallback(async (index: number) => {
		setActiveIndex(index);
		await tauriAPI.updateActiveSttProviderIndex(index);
	}, []);

	const updateProvider = useCallback((index: number, patch: Partial<SttProvider>) => {
		setProviders(prev => {
			const updated = [...prev];
			updated[index] = { ...updated[index], ...patch } as SttProvider;
			tauriAPI.updateSttProviders(updated);
			return updated;
		});
	}, []);

	const addProvider = useCallback(() => {
		const newProvider: SttProvider = {
			name: `Provider ${providers.length + 1}`,
			base_url: "https://openrouter.ai/api/v1/audio/transcriptions",
			model: "openai/whisper-large-v3",
			api_key: "",
			request_format: "json",
			extra_body: '{"provider":{"only":["groq"]}}',
		};
		saveProviders([...providers, newProvider]);
	}, [providers, saveProviders]);

	const removeProvider = useCallback((index: number) => {
		if (providers.length <= 1) return;
		const updated = providers.filter((_, i) => i !== index);
		const newActive = activeIndex >= updated.length ? updated.length - 1 : activeIndex > index ? activeIndex - 1 : activeIndex;
		setProviders(updated);
		setActiveIndex(newActive);
		tauriAPI.updateSttProviders(updated);
		tauriAPI.updateActiveSttProviderIndex(newActive);
	}, [providers, activeIndex]);

	const handleSaveOpenaiKey = async () => {
		await tauriAPI.updateOpenaiApiKey(openaiApiKey || null);
		setOpenaiSaved(true);
		setTimeout(() => setOpenaiSaved(false), 2000);
	};

	return (
		<Paper p="md" radius="md" className="animate-in" mb="md">
			<Title order={4} mb="xs">Speech-to-Text Providers</Title>
			<Text c="dimmed" size="sm" mb="md">
				Configure one or more STT providers. The active one is used for transcription.
			</Text>

			<Stack gap="sm">
				{providers.map((provider, index) => (
					<Paper
						key={index}
						p="sm"
						radius="sm"
						withBorder
						style={{
							borderColor: index === activeIndex ? "var(--mantine-color-blue-6)" : undefined,
							borderWidth: index === activeIndex ? 2 : 1,
						}}
					>
						<Group justify="space-between" mb="xs">
							<Radio
								checked={index === activeIndex}
								onChange={() => saveActiveIndex(index)}
								label={provider.name || `Provider ${index + 1}`}
								styles={{ label: { fontWeight: 600 } }}
							/>
							{providers.length > 1 && (
								<ActionIcon
									variant="subtle"
									color="red"
									size="sm"
									onClick={() => removeProvider(index)}
									title="Remove provider"
								>
									✕
								</ActionIcon>
							)}
						</Group>

						<TextInput
							label="Name"
							size="xs"
							value={provider.name}
							onChange={(e) => updateProvider(index, { name: e.currentTarget.value })}
							mb="xs"
						/>

						<PasswordInput
							label="API Key"
							size="xs"
							placeholder="sk-..."
							value={provider.api_key}
							onChange={(e) => updateProvider(index, { api_key: e.currentTarget.value })}
							mb="xs"
						/>

						<TextInput
							label="Base URL"
							size="xs"
							placeholder={DEFAULT_PROVIDER.base_url}
							value={provider.base_url}
							onChange={(e) => updateProvider(index, { base_url: e.currentTarget.value })}
							mb="xs"
						/>

						<Group grow mb="xs">
							<TextInput
								label="Model"
								size="xs"
								placeholder="whisper-1"
								value={provider.model}
								onChange={(e) => updateProvider(index, { model: e.currentTarget.value })}
							/>
							<Select
								label="Format"
								size="xs"
								data={[
									{ value: "multipart", label: "Multipart (OpenAI)" },
									{ value: "json", label: "JSON (OpenRouter)" },
									{ value: "gemini", label: "Gemini (Google)" },
								]}
								value={provider.request_format}
								onChange={(val) => updateProvider(index, { request_format: val || "multipart" })}
							/>
						</Group>

						<Textarea
							label="Extra Body (JSON)"
							size="xs"
							placeholder='{"provider": {"only": ["groq"]}}'
							value={provider.extra_body ?? ""}
							onChange={(e) => updateProvider(index, { extra_body: e.currentTarget.value || null })}
							autosize
							minRows={1}
							maxRows={3}
						/>
					</Paper>
				))}
			</Stack>

			<Button onClick={addProvider} size="xs" variant="light" mt="sm">
				+ Add Provider
			</Button>

			<Divider my="lg" />

			<Title order={4} mb="xs">STT Prompt</Title>
			<Text c="dimmed" size="sm" mb="md">
				Context hint sent to the STT model with every request. Helps with terminology, names, abbreviations.
			</Text>
			<Textarea
				placeholder="Terminology: React, TypeScript, Tauri, OpenRouter, Whisper..."
				value={sttPrompt}
				onChange={(e) => setSttPrompt(e.currentTarget.value)}
				onBlur={() => tauriAPI.updateSttPrompt(sttPrompt || null)}
				autosize
				minRows={2}
				maxRows={5}
				mb="sm"
			/>

			<Divider my="lg" />

			<Title order={4} mb="xs">Paste Prefix</Title>
			<Text c="dimmed" size="sm" mb="md">
				Optional text prepended to every transcription before pasting (e.g. "🎙 ").
			</Text>
			<TextInput
				placeholder="🎙 "
				value={pastePrefix}
				onChange={(e) => setPastePrefix(e.currentTarget.value)}
				onBlur={() => tauriAPI.updatePastePrefix(pastePrefix || null)}
				mb="sm"
			/>

			<Divider my="lg" />

			<Title order={4} mb="xs">LLM API Key</Title>
			<Text c="dimmed" size="sm" mb="md">
				OpenAI API key for optional LLM text formatting.
			</Text>

			<PasswordInput
				label="OpenAI API Key"
				placeholder="sk-..."
				value={openaiApiKey}
				onChange={(e) => setOpenaiApiKey(e.currentTarget.value)}
				mb="sm"
			/>

			<Button onClick={handleSaveOpenaiKey} size="sm" variant="light">
				{openaiSaved ? "Saved!" : "Save"}
			</Button>
		</Paper>
	);
}
