import React from 'react';
import { UserButton } from '@clerk/clerk-react';
import { useInviteCode } from '@/hooks/useInviteCode';
import { Copy } from 'lucide-react';
import { toast } from 'sonner';
import { useOnboarding } from '@/hooks/useOnboarding';

const CustomUserButton: React.FC = () => {
    const { inviteCode, isLoading } = useInviteCode();
    const { isWaitlisted } = useOnboarding();

    const copyInviteCode = async () => {
        if (inviteCode) {
            await navigator.clipboard.writeText(inviteCode);
            toast.success('Invite code copied to clipboard');
        }
    };

    return (
        <UserButton>
            <UserButton.MenuItems>
                {!isWaitlisted && inviteCode && !isLoading && (
                    <UserButton.Action
                        label={`Invite Code: ${inviteCode}`}
                        labelIcon={<Copy className="h-4 w-4" />}
                        onClick={copyInviteCode}
                    />
                )}
                <UserButton.Action label="manageAccount" />
                <UserButton.Action label="signOut" />
            </UserButton.MenuItems>
        </UserButton>
    );
};

export default CustomUserButton; 