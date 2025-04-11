import { useOnboarding } from "@/hooks/useOnboarding";
import { LoadingScreen } from "./LoadingScreen";
import { Navigate } from "react-router-dom";

interface InvitedOnlyProps {
    children: React.ReactNode;
}

export function InvitedOnly({ children }: InvitedOnlyProps) {
    const { isLoading, isWaitlisted } = useOnboarding();

    if (isLoading) {
        return <LoadingScreen />;
    }

    if (isWaitlisted) {
        return <Navigate to="/invitation" replace />;
    }

    return <>{children}</>;
}
