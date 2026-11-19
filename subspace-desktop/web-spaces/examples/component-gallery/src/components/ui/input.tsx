// Locally owned shadcn/ui Input. See public/licences/shadcn-MIT.txt.
import type { ComponentProps } from 'react';
import { cn } from '@/lib/utils';

function Input({ className, type, ...props }: ComponentProps<'input'>) {
  return (
    <input data-slot="input" type={type}
      className={cn('gallery-input flex w-full min-w-0 border bg-transparent outline-none focus-visible:border-[var(--ring)] focus-visible:outline-2 focus-visible:outline-[var(--ring)] disabled:cursor-not-allowed', className)}
      {...props} />
  );
}

export { Input };
