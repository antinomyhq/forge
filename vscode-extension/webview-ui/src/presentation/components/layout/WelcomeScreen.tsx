import React from "react";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Sparkles, Code2, FileSearch, Lightbulb } from "lucide-react";

interface WelcomeScreenProps {
  onQuickAction?: (action: string) => void;
}

/// WelcomeScreen displays an empty state with quick action suggestions
export const WelcomeScreen: React.FC<WelcomeScreenProps> = ({ onQuickAction }) => {
  const quickActions = [
    {
      icon: <Code2 className="h-5 w-5" />,
      title: "Write Code",
      description: "Generate code snippets and functions",
      action: "Write a function to...",
    },
    {
      icon: <FileSearch className="h-5 w-5" />,
      title: "Review Code",
      description: "Get feedback on your code",
      action: "Review this code for...",
    },
    {
      icon: <Lightbulb className="h-5 w-5" />,
      title: "Explain Concept",
      description: "Learn programming concepts",
      action: "Explain how...",
    },
  ];

  return (
    <div className="container max-w-3xl px-4 py-8">
      <Card className="border-none shadow-none bg-transparent">
        <CardHeader className="text-center space-y-4 pb-8">
          <div className="mx-auto flex h-16 w-16 items-center justify-center rounded-full bg-primary/10">
            <Sparkles className="h-8 w-8 text-primary" />
          </div>
          <div>
            <CardTitle className="text-3xl font-bold mb-2">
              Welcome to ForgeCode
            </CardTitle>
            <CardDescription className="text-base">
              Your AI-powered coding assistant. Start a conversation to get help with your code.
            </CardDescription>
          </div>
        </CardHeader>

        <CardContent className="space-y-4">
          <div className="text-sm font-medium text-muted-foreground text-center mb-4">
            Try one of these quick actions:
          </div>
          
          <div className="grid gap-3 sm:grid-cols-3">
            {quickActions.map((item, index) => (
              <Card
                key={index}
                className="cursor-pointer transition-all hover:border-primary hover:shadow-md"
                onClick={() => onQuickAction?.(item.action)}
              >
                <CardHeader className="pb-3">
                  <div className="flex items-center gap-2">
                    <div className="flex h-8 w-8 items-center justify-center rounded-md bg-primary/10 text-primary">
                      {item.icon}
                    </div>
                    <CardTitle className="text-sm font-semibold">
                      {item.title}
                    </CardTitle>
                  </div>
                </CardHeader>
                <CardContent className="pb-4">
                  <CardDescription className="text-xs">
                    {item.description}
                  </CardDescription>
                </CardContent>
              </Card>
            ))}
          </div>

          <div className="text-center pt-4">
            <p className="text-xs text-muted-foreground">
              Or type your own message below to get started
            </p>
          </div>
        </CardContent>
      </Card>
    </div>
  );
};
