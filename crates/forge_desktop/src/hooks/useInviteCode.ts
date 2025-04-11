import { useState, useEffect } from 'react';
import { useAuth } from '@clerk/clerk-react';

interface InviteCodeResponse {
    code: string;
    userId: string;
    usedBy: string | null;
}

interface ErrorResponse {
    message: string;
}

export function useInviteCode() {
    const [inviteCode, setInviteCode] = useState<string | null>(null);
    const [isLoading, setIsLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);
    const { getToken } = useAuth();

    useEffect(() => {
        const fetchInviteCode = async () => {
            try {
                const token = await getToken();
                const response = await fetch(`${import.meta.env.VITE_BACKEND_API_URL}/api/v1/invite`, {
                    headers: {
                        'Authorization': `Bearer ${token}`,
                    },
                });

                if (!response.ok) {
                    const errorData: ErrorResponse = await response.json().catch(() => ({ 
                        message: `HTTP error! status: ${response.status}` 
                    }));
                    throw new Error(errorData.message);
                }

                const data: InviteCodeResponse = await response.json();
                setInviteCode(data.code);
            } catch (err) {
                setError(err instanceof Error ? err.message : 'Failed to fetch invite code');
                setInviteCode(null);
            } finally {
                setIsLoading(false);
            }
        };

        fetchInviteCode();
    }, [getToken]);

    return {
        inviteCode,
        isLoading,
        error
    };
}
