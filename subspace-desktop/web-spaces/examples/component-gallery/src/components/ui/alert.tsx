// Locally owned shadcn/ui Alert composition. See public/licences/shadcn-MIT.txt.
import { cva, type VariantProps } from 'class-variance-authority';
import type { ComponentProps } from 'react';
import { cn } from '@/lib/utils';

const alertVariants = cva('relative w-full', {
  variants: { variant: { default: 'text-[var(--foreground)]', destructive: 'text-[var(--danger-background)]' } },
  defaultVariants: { variant: 'default' },
});

function Alert({ className, variant, ...props }: ComponentProps<'div'> & VariantProps<typeof alertVariants>) {
  return <div data-slot="alert" role="alert" className={cn(alertVariants({ variant }), className)} {...props} />;
}

function AlertTitle({ className, ...props }: ComponentProps<'h3'>) {
  return <h3 data-slot="alert-title" className={cn('font-normal', className)} {...props} />;
}

function AlertDescription({ className, ...props }: ComponentProps<'div'>) {
  return <div data-slot="alert-description" className={cn('text-[var(--muted-foreground)]', className)} {...props} />;
}

export { Alert, AlertTitle, AlertDescription };
