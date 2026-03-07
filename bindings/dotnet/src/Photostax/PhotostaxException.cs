namespace Photostax;

/// <summary>
/// Exception thrown when a Photostax operation fails.
/// </summary>
public class PhotostaxException : Exception
{
    /// <summary>
    /// Initializes a new instance of the <see cref="PhotostaxException"/> class.
    /// </summary>
    public PhotostaxException()
    {
    }

    /// <summary>
    /// Initializes a new instance of the <see cref="PhotostaxException"/> class with a message.
    /// </summary>
    /// <param name="message">The error message.</param>
    public PhotostaxException(string message) : base(message)
    {
    }

    /// <summary>
    /// Initializes a new instance of the <see cref="PhotostaxException"/> class with a message and inner exception.
    /// </summary>
    /// <param name="message">The error message.</param>
    /// <param name="innerException">The inner exception.</param>
    public PhotostaxException(string message, Exception innerException) : base(message, innerException)
    {
    }
}
