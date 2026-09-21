import type { ReactNode } from "react";
import { cn } from "@/lib/utils";
import { Card, CardContent } from "@/components/ui/card";

export function PanelHeader({
  icon,
  title,
  subtitle,
  children,
  className,
}: {
  icon?: ReactNode;
  title: ReactNode;
  subtitle?: ReactNode;
  children?: ReactNode;
  className?: string;
}) {
  return (
    <div
      className={cn(
        "flex flex-col gap-3 md:flex-row md:items-center md:justify-between",
        className,
      )}
    >
      <div className="flex items-center gap-3">
        {icon && (
          <div className="rounded-xl bg-primary/10 p-2.5 text-primary">
            {icon}
          </div>
        )}
        <div>
          <h2 className="text-lg font-semibold leading-tight tracking-tight">
            {title}
          </h2>
          {subtitle && (
            <p className="mt-0.5 text-xs text-muted-foreground">{subtitle}</p>
          )}
        </div>
      </div>
      {children && (
        <div className="flex flex-wrap items-center gap-1.5">{children}</div>
      )}
    </div>
  );
}

export function EmptyState({
  icon,
  message,
  children,
}: {
  icon?: ReactNode;
  message: ReactNode;
  children?: ReactNode;
}) {
  return (
    <Card>
      <CardContent className="flex flex-col items-center gap-3 py-14 text-center">
        {icon && <div className="text-muted-foreground/30">{icon}</div>}
        <p className="text-sm text-muted-foreground">{message}</p>
        {children}
      </CardContent>
    </Card>
  );
}
