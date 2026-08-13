import { ActionIcon, Button, Group, Menu, Modal, Text } from "@mantine/core";
import { useClipboard, useDisclosure } from "@mantine/hooks";
import { format, isToday, isYesterday } from "date-fns";
import {
	Copy,
	Eye,
	EyeOff,
	MessageSquare,
	MoreVertical,
	RefreshCw,
	Trash2,
} from "lucide-react";
import { memo, useCallback, useEffect, useMemo, useState } from "react";
import type { HistoryEntry } from "../lib/tauri";
import { tauriAPI } from "../lib/tauri";

function formatTime(timestamp: string): string {
	return format(new Date(timestamp), "h:mm a");
}

function formatDate(timestamp: string): string {
	const date = new Date(timestamp);
	if (isToday(date)) return "Today";
	if (isYesterday(date)) return "Yesterday";
	return format(date, "MMM d");
}

function normalizeMeaningfulContextValue(
	rawContextValue: string | null | undefined,
): string | null {
	const trimmedContextValue = rawContextValue?.trim();
	return trimmedContextValue ? trimmedContextValue : null;
}

interface GroupedHistory {
	date: string;
	items: HistoryEntry[];
}

function groupHistoryByDate(history: HistoryEntry[]): GroupedHistory[] {
	const groups: Record<string, GroupedHistory> = {};

	for (const item of history) {
		const dateKey = formatDate(item.timestamp);
		if (!groups[dateKey]) {
			groups[dateKey] = { date: dateKey, items: [] };
		}
		groups[dateKey].items.push(item);
	}

	return Object.values(groups);
}

interface HistoryItemProps {
	entry: HistoryEntry;
	onCopy: (text: string) => void;
	onDelete: (id: string) => void;
	isDeleting: boolean;
	canRetranscribe: boolean;
	isRetranscribing: boolean;
	onRetranscribe: () => void;
}

const HistoryItem = memo(function HistoryItem({
	entry,
	onCopy,
	onDelete,
	isDeleting,
	canRetranscribe,
	isRetranscribing,
	onRetranscribe,
}: HistoryItemProps) {
	const [isExpanded, setIsExpanded] = useState(false);
	const activeAppContextSnapshot = entry.active_app_context;
	const activeAppContextFields = [
		{
			label: "App",
			value: normalizeMeaningfulContextValue(
				activeAppContextSnapshot?.focused_application?.display_name,
			),
		},
		{
			label: "Window",
			value: normalizeMeaningfulContextValue(
				activeAppContextSnapshot?.focused_window?.title,
			),
		},
		{
			label: "Tab",
			value: normalizeMeaningfulContextValue(
				activeAppContextSnapshot?.focused_browser_tab?.title,
			),
		},
		{
			label: "Origin",
			value: normalizeMeaningfulContextValue(
				activeAppContextSnapshot?.focused_browser_tab?.origin,
			),
		},
	].filter((activeAppContextField) => activeAppContextField.value !== null);

	return (
		<div className="history-item">
			<span className="history-time">{formatTime(entry.timestamp)}</span>
			<div className="history-content">
				{entry.status === "failed" ? (
					<p
						className="history-text"
						style={{ color: "var(--mantine-color-red-5)" }}
					>
						Transcription failed
						{entry.error ? `: ${entry.error}` : ""}
					</p>
				) : (
					<p className="history-text">{entry.text}</p>
				)}
				{isExpanded && (
					<div className="history-raw-text">
						<Text className="history-details-heading" mb={4}>
							Raw transcription:
						</Text>
						<Text className="history-details-content">{entry.raw_text}</Text>

						{activeAppContextSnapshot && (
							<>
								<Text className="history-details-heading" mt={12} mb={4}>
									Active app context:
								</Text>
								{activeAppContextFields.length > 0 ? (
									<div className="history-context-grid">
										{activeAppContextFields.map(({ label, value }) => (
											<div key={label} className="history-context-row">
												<Text className="history-context-label">{label}</Text>
												<Text className="history-context-value">{value}</Text>
											</div>
										))}
									</div>
								) : (
									<Text className="history-details-content">
										No active app details were available for this dictation.
									</Text>
								)}
							</>
						)}
					</div>
				)}
			</div>
			<div className="history-actions">
				<Menu shadow="md" width={180} position="bottom-end">
					<Menu.Target>
						<ActionIcon variant="subtle" size="sm" color="gray">
							<MoreVertical size={14} />
						</ActionIcon>
					</Menu.Target>
					<Menu.Dropdown>
						<Menu.Item
							leftSection={<Copy size={14} />}
							onClick={() => onCopy(entry.text)}
						>
							Copy
						</Menu.Item>
						<Menu.Item
							leftSection={<Copy size={14} />}
							onClick={() => onCopy(entry.raw_text)}
						>
							Copy raw
						</Menu.Item>
						{canRetranscribe && (
							<Menu.Item
								leftSection={<RefreshCw size={14} />}
								onClick={onRetranscribe}
								disabled={isRetranscribing}
							>
								{isRetranscribing ? "Re-transcribing..." : "Re-transcribe"}
							</Menu.Item>
						)}
						<Menu.Item
							leftSection={
								isExpanded ? <EyeOff size={14} /> : <Eye size={14} />
							}
							onClick={() => setIsExpanded((currentState) => !currentState)}
						>
							{isExpanded ? "Hide" : "View"} details
						</Menu.Item>
						<Menu.Divider />
						<Menu.Item
							color="red"
							leftSection={<Trash2 size={14} />}
							onClick={() => onDelete(entry.id)}
							disabled={isDeleting}
						>
							Delete
						</Menu.Item>
					</Menu.Dropdown>
				</Menu>
			</div>
		</div>
	);
});

export function HistoryFeed() {
	const [history, setHistory] = useState<HistoryEntry[]>([]);
	const [isLoading, setIsLoading] = useState(true);
	const [error, setError] = useState<string | null>(null);
	const [isDeleting, setIsDeleting] = useState(false);
	const [isClearing, setIsClearing] = useState(false);
	const [lastRecordingEntryId, setLastRecordingEntryId] = useState<
		string | null
	>(null);
	const [isRetranscribing, setIsRetranscribing] = useState(false);
	const clipboard = useClipboard();
	const [confirmOpened, { open: openConfirm, close: closeConfirm }] =
		useDisclosure(false);

	const loadHistory = useCallback(async () => {
		try {
			const [entries, lastEntryId] = await Promise.all([
				tauriAPI.getHistory(100),
				tauriAPI.getLastRecordingEntryId().catch(() => null),
			]);
			setHistory(entries);
			setLastRecordingEntryId(lastEntryId);
			setError(null);
		} catch {
			setError("Failed to load history");
		} finally {
			setIsLoading(false);
		}
	}, []);

	useEffect(() => {
		loadHistory();

		let unlisten: (() => void) | undefined;
		tauriAPI.onHistoryChanged(() => {
			loadHistory();
		}).then((fn) => {
			unlisten = fn;
		});

		return () => {
			unlisten?.();
		};
	}, [loadHistory]);

	const handleDelete = useCallback(
		async (id: string) => {
			setIsDeleting(true);
			try {
				await tauriAPI.deleteHistoryEntry(id);
				await loadHistory();
			} finally {
				setIsDeleting(false);
			}
		},
		[loadHistory],
	);

	const handleCopy = useCallback(
		(text: string) => {
			clipboard.copy(text);
		},
		[clipboard],
	);

	const handleRetranscribe = useCallback(async () => {
		setIsRetranscribing(true);
		try {
			await tauriAPI.retranscribeLast();
		} catch (retranscribeError) {
			console.error("Re-transcription failed:", retranscribeError);
		} finally {
			setIsRetranscribing(false);
		}
	}, []);

	const handleClearAll = async () => {
		setIsClearing(true);
		try {
			await tauriAPI.clearHistory();
			await loadHistory();
			closeConfirm();
		} finally {
			setIsClearing(false);
		}
	};

	const groupedHistory = useMemo(
		() => groupHistoryByDate(history),
		[history],
	);

	if (isLoading) {
		return (
			<div className="animate-in animate-in-delay-2">
				<div className="section-header">
					<span className="section-title">History</span>
				</div>
				<div className="empty-state">
					<p className="empty-state-text">Loading history...</p>
				</div>
			</div>
		);
	}

	if (error) {
		return (
			<div className="animate-in animate-in-delay-2">
				<div className="section-header">
					<span className="section-title">History</span>
				</div>
				<div className="empty-state">
					<p className="empty-state-text" style={{ color: "#ef4444" }}>
						Failed to load history
					</p>
				</div>
			</div>
		);
	}

	if (history.length === 0) {
		return (
			<div className="animate-in animate-in-delay-2">
				<div className="section-header">
					<span className="section-title">History</span>
				</div>
				<div className="empty-state">
					<MessageSquare className="empty-state-icon" />
					<h4 className="empty-state-title">No dictation history yet</h4>
					<p className="empty-state-text">
						Your transcribed text will appear here after you use voice
						dictation.
					</p>
				</div>
			</div>
		);
	}

	return (
		<div className="animate-in animate-in-delay-2">
			<div className="section-header">
				<span className="section-title">History</span>
				<Button
					variant="subtle"
					size="compact-sm"
					color="gray"
					onClick={openConfirm}
					disabled={isClearing}
				>
					Clear All
				</Button>
			</div>

			<Modal
				opened={confirmOpened}
				onClose={closeConfirm}
				title="Clear History"
				centered
				size="sm"
			>
				<Text size="sm" mb="lg">
					Are you sure you want to clear all history? This action cannot be
					undone.
				</Text>
				<Group justify="flex-end">
					<Button variant="default" onClick={closeConfirm}>
						Cancel
					</Button>
					<Button
						color="red"
						onClick={handleClearAll}
						loading={isClearing}
					>
						Clear All
					</Button>
				</Group>
			</Modal>

			{groupedHistory.map((group) => (
				<div key={group.date} style={{ marginBottom: 24 }}>
					<p
						className="section-title"
						style={{ marginBottom: 12, fontSize: 11 }}
					>
						{group.date}
					</p>
					<div className="history-feed">
						{group.items.map((entry) => (
							<HistoryItem
								key={entry.id}
								entry={entry}
								onCopy={handleCopy}
								onDelete={handleDelete}
								isDeleting={isDeleting}
								canRetranscribe={entry.id === lastRecordingEntryId}
								isRetranscribing={isRetranscribing}
								onRetranscribe={handleRetranscribe}
							/>
						))}
					</div>
				</div>
			))}
		</div>
	);
}
