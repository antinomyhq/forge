import { useEffect, useState } from 'react';
import { useAuth } from '@clerk/clerk-react';

type OnboardingStatus = 'WAITLISTED' | 'ACTIVE';
const API_URL = import.meta.env.VITE_BACKEND_API_URL;

interface OnboardingResponse {
  status: OnboardingStatus;
  createdAt: string;
  updatedAt: string;
}

export function useOnboarding() {
  const { getToken } = useAuth();
  const [status, setStatus] = useState<OnboardingStatus | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);

  useEffect(() => {
    const checkOnboardingStatus = async () => {
      try {
        const token = await getToken();
        const response = await fetch(`${API_URL}/api/v1/user/onboarding`, {
          headers: {
            'Authorization': `Bearer ${token}`,
          },
        });

        if (!response.ok) {
          const errorData = await response.json().catch(() => ({}));
          throw new Error(errorData.message || `Failed to fetch onboarding status: ${response.status}`);
        }

        const data: OnboardingResponse = await response.json();
        setStatus(data.status);
      } catch (err) {
        setError(err instanceof Error ? err : new Error('Unknown error'));
      } finally {
        setIsLoading(false);
      }
    };

    checkOnboardingStatus();
  }, [getToken]);

  return {
    status,
    isLoading,
    error,
    isWaitlisted: status === 'WAITLISTED',
  };
} 