package logger

import (
	"io"
	"os"

	"github.com/sirupsen/logrus"
)

var Log *logrus.Logger

// Init initializes the logger with both console and file output.
func Init(logFilePath string) error {
	Log = logrus.New()

	// Set log level
	Log.SetLevel(logrus.DebugLevel)

	// Create log file
	logFile, err := os.OpenFile(logFilePath, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0644)
	if err != nil {
		return err
	}

	// Create multi-writer for console and file output
	multiWriter := io.MultiWriter(os.Stdout, logFile)
	Log.SetOutput(multiWriter)

	// Set log format
	Log.SetFormatter(&logrus.TextFormatter{
		FullTimestamp: true,
	})

	return nil
}
