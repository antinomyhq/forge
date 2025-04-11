import { SignIn } from "@clerk/clerk-react";
import { CardContent } from "./ui/card";

export function LoginPage() {
  return (
    <div className="min-h-screen w-full flex items-center justify-center bg-background">
      <CardContent>
        <SignIn
          routing="path"
          path="/sign-in"
        />
      </CardContent>
    </div>
  );
} 