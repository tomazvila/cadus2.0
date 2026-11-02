import { createContext, useContext } from 'react';

/**
 * The move to the list of the own reports. The root app provides it. A panel outside the
 * app, such as a test mount, gets null and shows no link.
 */
export const OpenReportsContext = createContext<(() => void) | null>(null);

export function useOpenReports(): (() => void) | null {
  return useContext(OpenReportsContext);
}
