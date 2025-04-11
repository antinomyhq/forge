import { useUser } from "@clerk/clerk-react";
import { Navigate } from "react-router-dom";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Loader2, Sparkles } from "lucide-react";
import { useOnboarding } from "@/hooks/useOnboarding";
import { useRedeemInviteCode } from "@/hooks/useRedeemInviteCode";
import { Input } from "@/components/ui/input";
import { LoadingScreen } from "@/components/LoadingScreen";
import CustomUserButton from "./CustomUserButton";

export function InvitationPage() {
    const { user, isSignedIn, isLoaded: isAuthLoaded } = useUser();
    const { isLoading: isCheckingStatus, isWaitlisted, error } = useOnboarding();
    const { value, onChange, onSubmit, error: inviteError, isLoading, isRedeemed } = useRedeemInviteCode();
    const isDisabled = !value.trim() || isLoading;

    if (!isAuthLoaded || isCheckingStatus) {
        return <LoadingScreen />;
    }

    if (!isSignedIn) {
        return <Navigate to="/sign-in" replace />;
    }

    if (error) {
        return (
            <div className="h-screen w-full flex items-center justify-center bg-gradient-to-b from-background to-background/80">
                <Card className="w-[90%] max-w-md mx-auto border-destructive/20">
                    <CardHeader>
                        <CardTitle className="text-destructive">Error</CardTitle>
                        <CardDescription>
                            Failed to check access status. Please try again later.
                        </CardDescription>
                    </CardHeader>
                </Card>
            </div>
        );
    }

    if (!isWaitlisted || isRedeemed) {
        return <Navigate to="/" replace />;
    }

    return (
        <div className="h-screen w-full flex flex-col bg-gradient-to-b from-background to-background/80">
            {/* Header - fixed height */}
            <div className="h-14 flex justify-end px-4 border-b border-border/50 bg-card/50 backdrop-blur-sm">
                <div className="flex items-center">
                    <CustomUserButton />
                </div>
            </div>

            {/* Main content - takes remaining height */}
            <div className="flex-1 flex items-center justify-center p-4 overflow-y-auto">
                <div className="w-[90%] max-w-md mx-auto">
                    <Card className="backdrop-blur-sm border-primary/10 shadow-lg shadow-primary/5">
                        <CardHeader className="text-center py-6">
                            <div className="mx-auto bg-primary/10 w-12 h-12 rounded-full flex items-center justify-center mb-4">
                                <Sparkles className="w-6 h-6 text-primary" />
                            </div>
                            <div className="space-y-1.5">
                                <CardTitle className="text-xl sm:text-2xl font-bold bg-gradient-to-br from-primary to-primary-foreground bg-clip-text text-transparent">
                                    Welcome to the Waitlist
                                </CardTitle>
                                <CardDescription className="text-sm sm:text-base">
                                    You're one step away from accessing our exclusive features
                                </CardDescription>
                            </div>
                        </CardHeader>

                        <CardContent className="space-y-4 px-4 pb-6">
                            <div className="bg-muted/50 rounded-lg p-3 sm:p-4 text-sm text-muted-foreground">
                                <p>
                                    Your email <span className="font-medium text-foreground">{user?.primaryEmailAddress?.emailAddress}</span> is
                                    currently on our waitlist. Enter your invite code below to gain immediate access.
                                </p>
                            </div>
                            {inviteError && (
                                <div className="text-sm text-destructive bg-destructive/10 p-3 rounded-md">
                                    {inviteError}
                                </div>
                            )}
                            <div className="space-y-3">
                                <Input
                                    className="h-10 sm:h-12 text-base sm:text-lg tracking-wider placeholder:text-muted-foreground/50"
                                    placeholder="Enter your invite code"
                                    value={value}
                                    onChange={onChange}
                                    disabled={isLoading}
                                />
                                <Button
                                    className="w-full h-10 sm:h-12 text-sm sm:text-base font-medium"
                                    onClick={onSubmit}
                                    disabled={isDisabled}
                                    size="lg"
                                >
                                    {isLoading ? (
                                        <>
                                            <Loader2 className="h-4 w-4 sm:h-5 sm:w-5 animate-spin mr-2" />
                                            Verifying...
                                        </>
                                    ) : (
                                        'Unlock Access'
                                    )}
                                </Button>
                            </div>
                        </CardContent>
                    </Card>
                </div>
            </div>
        </div>
    );
} 