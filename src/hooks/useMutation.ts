import { useCallback, useRef, useState } from "react";
import type { MutationStatus } from "../components/settings/StatusIndicator";

interface MutationResult<TArgs, TResult> {
	mutate: (args: TArgs) => void;
	mutateAsync: (args: TArgs) => Promise<TResult>;
	status: MutationStatus;
	isPending: boolean;
	error: Error | null;
}

export function useMutation<TArgs = void, TResult = void>(
	fn: (args: TArgs) => Promise<TResult>,
	options?: {
		onSuccess?: (result: TResult) => void;
		onError?: (error: Error) => void;
	},
): MutationResult<TArgs, TResult> {
	const [status, setStatus] = useState<MutationStatus>("idle");
	const [error, setError] = useState<Error | null>(null);
	const optionsRef = useRef(options);
	optionsRef.current = options;

	const mutateAsync = useCallback(
		async (args: TArgs): Promise<TResult> => {
			setStatus("pending");
			setError(null);
			try {
				const result = await fn(args);
				setStatus("success");
				optionsRef.current?.onSuccess?.(result);
				return result;
			} catch (e) {
				const err = e instanceof Error ? e : new Error(String(e));
				setError(err);
				setStatus("error");
				optionsRef.current?.onError?.(err);
				throw err;
			}
		},
		[fn],
	);

	const mutate = useCallback(
		(args: TArgs) => {
			mutateAsync(args).catch(() => {});
		},
		[mutateAsync],
	);

	return {
		mutate,
		mutateAsync,
		status,
		isPending: status === "pending",
		error,
	};
}
