import { useState } from 'react';
import { useAuth } from '@clerk/clerk-react';

interface ErrorResponse {
    message: string;
}

export function useRedeemInviteCode() {
    const [inviteCode, setInviteCode] = useState("");
    const [isSubmitting, setIsSubmitting] = useState(false);
    const [submitError, setSubmitError] = useState<string | null>(null);
    const [isRedeemed, setIsRedeemed] = useState(false);
    const { getToken } = useAuth();

    const handleSubmit = async () => {
        if (!inviteCode.trim()) return;

        setIsSubmitting(true);
        setSubmitError(null);

        try {
            const token = await getToken();
            const response = await fetch(`${import.meta.env.VITE_BACKEND_API_URL}/api/v1/invite/redeem`, {
                method: 'POST',
                headers: {
                    'Content-Type': 'application/json',
                    'Authorization': `Bearer ${token}`,
                },
                body: JSON.stringify({ inviteCode }),
            });

            if (!response.ok) {
                const errorData: ErrorResponse = await response.json().catch(() => ({ 
                    message: `Failed to redeem invite code: ${response.status}` 
                }));
                throw new Error(errorData.message);
            }
            setIsRedeemed(response.status === 200);
        } catch (err) {
            setSubmitError(err instanceof Error ? err.message : 'Failed to verify invite code');
        } finally {
            setIsSubmitting(false);
        }
    };

    return {
        value: inviteCode,
        onChange: (e: React.ChangeEvent<HTMLInputElement>) => {
            setInviteCode(e.target.value);
            setSubmitError(null);
            setIsRedeemed(false);
        },
        onSubmit: handleSubmit,
        error: submitError,
        isLoading: isSubmitting,
        isRedeemed,
    };
} 