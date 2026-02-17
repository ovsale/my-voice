import { Loader } from "@mantine/core";
import { Check, X } from "lucide-react";
import { useEffect, useState } from "react";

export type MutationStatus = "idle" | "pending" | "success" | "error";

interface StatusIndicatorProps {
	status: MutationStatus;
}

export function StatusIndicator({ status }: StatusIndicatorProps) {
	const [visible, setVisible] = useState(false);

	useEffect(() => {
		if (status === "success" || status === "error") {
			setVisible(true);
			const timer = setTimeout(() => setVisible(false), 1500);
			return () => clearTimeout(timer);
		}
		setVisible(status === "pending");
		return undefined;
	}, [status]);

	if (!visible) return null;

	if (status === "pending") {
		return <Loader size={12} color="gray" />;
	}
	if (status === "success") {
		return <Check size={12} color="var(--mantine-color-green-6)" />;
	}
	if (status === "error") {
		return <X size={12} color="var(--mantine-color-red-6)" />;
	}
	return null;
}
